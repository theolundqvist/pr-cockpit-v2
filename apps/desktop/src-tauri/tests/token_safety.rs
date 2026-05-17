use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    auth_detect_gh_token_impl, auth_list_accounts_impl, auth_oauth_device_poll_impl,
    auth_oauth_device_start_impl, auth_pat_save_impl, auth_switch_account_impl, AccountLocator,
    AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, OAuthDevicePollInput,
    OAuthDevicePollResult, OAuthDeviceStartInput, PatSaveInput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::Db;
use regex::Regex;
use sqlx::Row;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;

const GH_TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwx123456";
const OAUTH_TOKEN: &str = "gho_abcdefghijklmnopqrstuvwx123456";
const OAUTH_ROTATED_TOKEN: &str = "gho_zabcdefghijklmnopqrstuvwx12345";
const OAUTH_REFRESH_TOKEN: &str = "refresh_oauth_token_1234567890";
const PAT_TOKEN: &str = "github_pat_1234567890ABCDEFGHIJKLMNOP_1234567890ABCD";

#[derive(Default)]
struct FakeTokenStore {
    entries: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl TokenStore for FakeTokenStore {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError> {
        self.entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .insert((host.to_string(), login.to_string()), token.clone());
        Ok(())
    }

    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .get(&(host.to_string(), login.to_string()))
            .cloned())
    }

    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError> {
        self.entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .remove(&(host.to_string(), login.to_string()));
        Ok(())
    }
}

struct FakeGhCli;

impl GhCli for FakeGhCli {
    fn run(&self, args: &[&str]) -> Result<GhCommandOutput, AuthError> {
        match args {
            ["auth", "token"] => Ok(GhCommandOutput {
                status: 0,
                stdout: format!("{GH_TOKEN}\n"),
                stderr: String::new(),
            }),
            ["auth", "status", "--show-token"] => Ok(GhCommandOutput {
                status: 0,
                stdout: format!(
                    "github.com\n  ✓ Logged in to github.com as gh-user (keyring)\n  - Token: {GH_TOKEN}\n  - Token scopes: 'repo', 'read:org'\n"
                ),
                stderr: String::new(),
            }),
            _ => Err(AuthError::OAuthFlowFailed),
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn token_safety_guards_all_auth_paths() -> Result<()> {
    let capture = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer {
        events: Arc::clone(&capture),
    });
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::info!("starting token safety test");

    let mock_server = MockGithubServer::start().await?;

    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        Arc::new(FakeTokenStore::default()),
        Arc::new(FakeGhCli),
        reqwest::Client::new(),
        AuthConfig {
            github_web_origin: mock_server.base_url.clone(),
            github_api_origin: mock_server.base_url.clone(),
            ..AuthConfig::default()
        },
    ));

    let gh_import = auth_detect_gh_token_impl(auth.as_ref(), true)
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    assert!(gh_import.saved);

    let start = auth_oauth_device_start_impl(
        auth.as_ref(),
        OAuthDeviceStartInput {
            host: Some("github.com".to_string()),
            scopes: vec!["repo".to_string(), "read:org".to_string()],
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    let oauth_result = auth_oauth_device_poll_impl(
        auth.as_ref(),
        OAuthDevicePollInput {
            host: Some("github.com".to_string()),
            device_code: start.device_code.clone(),
            interval_seconds: start.interval_seconds,
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    assert!(matches!(
        oauth_result,
        OAuthDevicePollResult::Authorized { .. }
    ));

    let _pat_account = auth_pat_save_impl(
        auth.as_ref(),
        PatSaveInput {
            host: Some("github.com".to_string()),
            token: PAT_TOKEN.to_string(),
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err:?}"))?;

    let account_list = auth_list_accounts_impl(auth.as_ref())
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    assert_eq!(account_list.accounts.len(), 3);
    let switched = auth_switch_account_impl(
        auth.as_ref(),
        AccountLocator {
            host: "github.com".to_string(),
            login: "oauth-user".to_string(),
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    assert!(switched.is_active);

    let token_client = TokenClient::from_auth_service(Arc::clone(&auth));
    let sync_response = token_client
        .request(
            &AccountLocator {
                host: "github.com".to_string(),
                login: "oauth-user".to_string(),
            },
            reqwest::Method::GET,
            &format!("{}/headers", mock_server.base_url),
        )
        .await?;
    assert!(sync_response.status().is_success());

    let token_regex = Regex::new(
        r"(gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|refresh_[A-Za-z0-9_]{20,})",
    )?;
    let rows = sqlx::query("SELECT * FROM accounts")
        .fetch_all(db.pool())
        .await?;
    for row in rows {
        for (column_index, _) in row.columns().iter().enumerate() {
            if let Ok(value) = row.try_get::<String, _>(column_index) {
                assert!(
                    !token_regex.is_match(&value),
                    "token regex matched accounts.{column_index} value"
                );
            }
        }
    }

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await?;
    let db_path = temp.path().join("cockpit.db");
    let wal_path = temp.path().join("cockpit.db-wal");
    let db_bytes = tokio::fs::read(db_path).await?;
    let wal_bytes = match tokio::fs::read(wal_path).await {
        Ok(bytes) => bytes,
        Err(_) => Vec::new(),
    };
    for token in [
        GH_TOKEN,
        OAUTH_TOKEN,
        OAUTH_ROTATED_TOKEN,
        OAUTH_REFRESH_TOKEN,
        PAT_TOKEN,
    ] {
        assert!(
            !contains_bytes(&db_bytes, token.as_bytes()),
            "database file leaked token bytes"
        );
        assert!(
            !contains_bytes(&wal_bytes, token.as_bytes()),
            "wal file leaked token bytes"
        );
    }

    let events = capture
        .lock()
        .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?;
    for event in events.iter() {
        for token in [
            GH_TOKEN,
            OAUTH_TOKEN,
            OAUTH_ROTATED_TOKEN,
            OAUTH_REFRESH_TOKEN,
            PAT_TOKEN,
        ] {
            assert!(
                !event.contains(token),
                "tracing event leaked raw token content"
            );
        }
    }

    mock_server.shutdown().await;
    Ok(())
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

struct MockGithubServer {
    base_url: String,
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl MockGithubServer {
    async fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let base_url = format!("http://{}", address);
        let (tx, mut rx) = oneshot::channel::<()>();

        let join_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accept_result = listener.accept() => {
                        let Ok((stream, _)) = accept_result else {
                            break;
                        };
                        tokio::spawn(async move {
                            let _ = handle_connection(stream).await;
                        });
                    }
                }
            }
        });

        Ok(Self {
            base_url,
            shutdown: Some(tx),
            join_handle,
        })
    }

    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join_handle.await;
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let (method, path, headers, body) = read_http_request(&mut stream).await?;
    let authorization = headers.get("authorization").cloned().unwrap_or_default();

    let (status, extra_headers, response_body) = if method == "POST" && path == "/login/device/code"
    {
        (
            "200 OK",
            vec!["Content-Type: application/json".to_string()],
            r#"{"device_code":"device-code-1","user_code":"1234-ABCD","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#.to_string(),
        )
    } else if method == "POST" && path == "/login/oauth/access_token" {
        if body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
            && body.contains("device_code=device-code-1")
        {
            (
                "200 OK",
                vec!["Content-Type: application/json".to_string()],
                format!(
                    r#"{{"access_token":"{OAUTH_TOKEN}","scope":"repo,read:org","refresh_token":"{OAUTH_REFRESH_TOKEN}","refresh_token_expires_in":3600}}"#
                ),
            )
        } else if body.contains("grant_type=refresh_token")
            && body.contains(&format!("refresh_token={OAUTH_REFRESH_TOKEN}"))
        {
            (
                "200 OK",
                vec!["Content-Type: application/json".to_string()],
                format!(
                    r#"{{"access_token":"{OAUTH_ROTATED_TOKEN}","scope":"repo,read:org","refresh_token":"{OAUTH_REFRESH_TOKEN}","refresh_token_expires_in":3600}}"#
                ),
            )
        } else {
            (
                "400 Bad Request",
                vec!["Content-Type: application/json".to_string()],
                r#"{"error":"authorization_pending"}"#.to_string(),
            )
        }
    } else if method == "GET" && path == "/user" {
        match authorization.as_str() {
            value if value == format!("Bearer {GH_TOKEN}") => (
                "200 OK",
                vec![
                    "Content-Type: application/json".to_string(),
                    "X-OAuth-Scopes: repo,read:org".to_string(),
                ],
                r#"{"login":"gh-user"}"#.to_string(),
            ),
            value if value == format!("Bearer {OAUTH_TOKEN}") => (
                "200 OK",
                vec![
                    "Content-Type: application/json".to_string(),
                    "X-OAuth-Scopes: repo,read:org".to_string(),
                ],
                r#"{"login":"oauth-user"}"#.to_string(),
            ),
            value if value == format!("Bearer {PAT_TOKEN}") => (
                "200 OK",
                vec![
                    "Content-Type: application/json".to_string(),
                    "X-OAuth-Scopes: repo,read:org,notifications".to_string(),
                ],
                r#"{"login":"pat-user"}"#.to_string(),
            ),
            value if value == format!("Bearer {OAUTH_ROTATED_TOKEN}") => (
                "200 OK",
                vec![
                    "Content-Type: application/json".to_string(),
                    "X-OAuth-Scopes: repo,read:org".to_string(),
                ],
                r#"{"login":"oauth-user"}"#.to_string(),
            ),
            _ => (
                "401 Unauthorized",
                vec!["Content-Type: application/json".to_string()],
                r#"{"message":"unauthorized"}"#.to_string(),
            ),
        }
    } else if method == "GET" && path == "/headers" {
        if authorization == format!("Bearer {OAUTH_TOKEN}") {
            (
                "401 Unauthorized",
                vec!["Content-Type: application/json".to_string()],
                r#"{"message":"expired"}"#.to_string(),
            )
        } else if authorization == format!("Bearer {OAUTH_ROTATED_TOKEN}") {
            (
                "200 OK",
                vec!["Content-Type: application/json".to_string()],
                r#"{"ok":true}"#.to_string(),
            )
        } else {
            (
                "401 Unauthorized",
                vec!["Content-Type: application/json".to_string()],
                r#"{"message":"unauthorized"}"#.to_string(),
            )
        }
    } else {
        (
            "404 Not Found",
            vec!["Content-Type: application/json".to_string()],
            r#"{"message":"not found"}"#.to_string(),
        )
    };

    let mut response = format!("HTTP/1.1 {status}\r\n");
    for header in extra_headers {
        response.push_str(&format!("{header}\r\n"));
    }
    response.push_str(&format!("Content-Length: {}\r\n\r\n", response_body.len()));
    response.push_str(&response_body);
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn read_http_request(
    stream: &mut TcpStream,
) -> Result<(String, String, HashMap<String, String>, String)> {
    let mut buffer = vec![0_u8; 4096];
    let mut bytes_read = 0_usize;
    loop {
        let read_count = stream.read(&mut buffer[bytes_read..]).await?;
        if read_count == 0 {
            break;
        }
        bytes_read += read_count;
        if bytes_read >= 4
            && buffer[..bytes_read]
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
        {
            break;
        }
        if bytes_read == buffer.len() {
            buffer.resize(buffer.len() * 2, 0);
        }
    }
    let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
    let mut lines = request_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing request line"))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing method"))?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing path"))?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let header_end = request_text.find("\r\n\r\n").unwrap_or(request_text.len());
    let body_start = header_end.saturating_add(4);
    let mut body_bytes = buffer[body_start..bytes_read].to_vec();
    while body_bytes.len() < content_length {
        let mut next = vec![0_u8; content_length - body_bytes.len()];
        let read_count = stream.read(&mut next).await?;
        if read_count == 0 {
            break;
        }
        body_bytes.extend_from_slice(&next[..read_count]);
    }
    let body =
        String::from_utf8_lossy(&body_bytes[..content_length.min(body_bytes.len())]).to_string();

    Ok((method, path, headers, body))
}

#[derive(Clone)]
struct CaptureLayer {
    events: Arc<Mutex<Vec<String>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        if let Ok(mut events) = self.events.lock() {
            events.push(visitor.fields.join(" "));
        }
    }
}

#[derive(Default)]
struct EventFieldVisitor {
    fields: Vec<String>,
}

impl Visit for EventFieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.push(format!("{}={value:?}", field.name()));
    }
}
