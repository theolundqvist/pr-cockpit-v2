pub mod gh;
pub mod token_client;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::db::{AuthAccountRow, Db};

const DEFAULT_OAUTH_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEFAULT_HOST: &str = "github.com";

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AuthAccount {
    pub host: String,
    pub login: String,
    pub api_base_url: String,
    pub graphql_url: String,
    pub token_kind: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct EndpointConfig {
    pub api_base_url: String,
    pub graphql_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct EndpointOverride {
    pub api_base_url: String,
    pub graphql_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AccountLocator {
    pub host: String,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct GhDetectionResult {
    pub host: String,
    pub login: String,
    pub scopes: Vec<String>,
    pub source: String,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct OAuthDeviceStart {
    pub host: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct OAuthDeviceStartInput {
    pub host: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct OAuthDevicePollInput {
    pub host: Option<String>,
    pub device_code: String,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum OAuthDevicePollResult {
    Pending { interval_seconds: u64 },
    SlowDown { interval_seconds: u64 },
    Denied,
    Expired,
    Authorized { account: AuthAccount },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PatSaveInput {
    pub host: Option<String>,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AccountsListResponse {
    pub active: Option<AccountLocator>,
    pub accounts: Vec<AuthAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RefreshResult {
    pub refreshed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AuthErrorCode {
    GhMissing,
    GhNotLoggedIn,
    GhParseFailed,
    InvalidToken,
    AccountNotFound,
    KeyringUnavailable,
    OAuthDenied,
    OAuthExpired,
    OAuthFlowFailed,
    UnsupportedHost,
    HttpFailure,
    DatabaseFailure,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AuthCommandError {
    pub code: AuthErrorCode,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("GitHub CLI is not installed")]
    GhMissing,
    #[error("GitHub CLI is not logged in")]
    GhNotLoggedIn,
    #[error("failed to parse GitHub CLI auth output")]
    GhParseFailed,
    #[error("account not found")]
    AccountNotFound,
    #[error("token validation failed")]
    InvalidToken,
    #[error("OAuth device flow denied by user")]
    OAuthDenied,
    #[error("OAuth device flow expired")]
    OAuthExpired,
    #[error("OAuth device flow failed")]
    OAuthFlowFailed,
    #[error("host is not supported for this auth flow")]
    UnsupportedHost,
    #[error("keyring unavailable")]
    KeyringUnavailable,
    #[error("HTTP call failed")]
    Http(#[from] reqwest::Error),
    #[error("HTTP status failed")]
    HttpStatus(reqwest::StatusCode),
    #[error("database failure")]
    Db(#[from] anyhow::Error),
    #[error("serialization failure")]
    Serde(#[from] serde_json::Error),
    #[error("invalid header value")]
    HeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error("system clock failure")]
    Time,
}

impl From<AuthError> for AuthCommandError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::GhMissing => Self {
                code: AuthErrorCode::GhMissing,
                message: "GitHub CLI is not installed.".to_string(),
            },
            AuthError::GhNotLoggedIn => Self {
                code: AuthErrorCode::GhNotLoggedIn,
                message: "GitHub CLI is installed but no account is logged in.".to_string(),
            },
            AuthError::GhParseFailed => Self {
                code: AuthErrorCode::GhParseFailed,
                message: "Failed to parse GitHub CLI auth output.".to_string(),
            },
            AuthError::InvalidToken => Self {
                code: AuthErrorCode::InvalidToken,
                message: "The provided token is invalid for the selected host.".to_string(),
            },
            AuthError::AccountNotFound => Self {
                code: AuthErrorCode::AccountNotFound,
                message: "The requested account could not be found.".to_string(),
            },
            AuthError::KeyringUnavailable => Self {
                code: AuthErrorCode::KeyringUnavailable,
                message: "OS keychain is unavailable for secure token storage.".to_string(),
            },
            AuthError::OAuthDenied => Self {
                code: AuthErrorCode::OAuthDenied,
                message: "OAuth authorization was denied.".to_string(),
            },
            AuthError::OAuthExpired => Self {
                code: AuthErrorCode::OAuthExpired,
                message: "OAuth authorization expired.".to_string(),
            },
            AuthError::OAuthFlowFailed => Self {
                code: AuthErrorCode::OAuthFlowFailed,
                message: "OAuth flow did not complete.".to_string(),
            },
            AuthError::UnsupportedHost => Self {
                code: AuthErrorCode::UnsupportedHost,
                message: "This auth flow is only supported on github.com.".to_string(),
            },
            AuthError::Http(_) | AuthError::HttpStatus(_) => Self {
                code: AuthErrorCode::HttpFailure,
                message: "GitHub API request failed.".to_string(),
            },
            AuthError::Db(_) => Self {
                code: AuthErrorCode::DatabaseFailure,
                message: "Failed to persist account metadata.".to_string(),
            },
            AuthError::Serde(_) | AuthError::HeaderValue(_) | AuthError::Time => Self {
                code: AuthErrorCode::Internal,
                message: "Internal auth failure.".to_string(),
            },
        }
    }
}

pub trait TokenStore: Send + Sync {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError>;
    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError>;
    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError>;
}

#[derive(Debug, Clone)]
pub struct KeyringTokenStore {
    service_prefix: String,
}

impl KeyringTokenStore {
    pub fn new(service_prefix: impl Into<String>) -> Self {
        Self {
            service_prefix: service_prefix.into(),
        }
    }

    fn entry(&self, host: &str, login: &str) -> Result<keyring::Entry, AuthError> {
        let service = format!("{}/{}", self.service_prefix, host);
        keyring::Entry::new(&service, login).map_err(|_| AuthError::KeyringUnavailable)
    }
}

impl Default for KeyringTokenStore {
    fn default() -> Self {
        Self::new("pr-cockpit")
    }
}

impl TokenStore for KeyringTokenStore {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError> {
        let payload = serde_json::to_string(token)?;
        self.entry(host, login)?
            .set_password(&payload)
            .map_err(|_| AuthError::KeyringUnavailable)
    }

    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError> {
        match self.entry(host, login)?.get_password() {
            Ok(payload) => Ok(Some(serde_json::from_str(&payload)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AuthError::KeyringUnavailable),
        }
    }

    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError> {
        match self.entry(host, login)?.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AuthError::KeyringUnavailable),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredTokenSecret {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhCommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl GhCommandOutput {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

pub trait GhCli: Send + Sync {
    fn run(&self, args: &[&str]) -> Result<GhCommandOutput, AuthError>;
}

#[derive(Debug, Default)]
pub struct SystemGhCli;

impl GhCli for SystemGhCli {
    fn run(&self, args: &[&str]) -> Result<GhCommandOutput, AuthError> {
        let output = std::process::Command::new("gh")
            .args(args)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    AuthError::GhMissing
                } else {
                    AuthError::OAuthFlowFailed
                }
            })?;
        Ok(GhCommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub oauth_client_id: String,
    pub github_web_origin: String,
    pub github_api_origin: String,
    pub default_host: String,
    pub endpoint_overrides_path: Option<PathBuf>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            oauth_client_id: DEFAULT_OAUTH_CLIENT_ID.to_string(),
            github_web_origin: "https://github.com".to_string(),
            github_api_origin: "https://api.github.com".to_string(),
            default_host: DEFAULT_HOST.to_string(),
            endpoint_overrides_path: default_endpoint_overrides_path(),
        }
    }
}

impl AuthConfig {
    fn web_origin(&self, host: &str) -> String {
        if host.eq_ignore_ascii_case(DEFAULT_HOST) {
            self.github_web_origin.clone()
        } else {
            format!("https://{host}")
        }
    }

    fn default_endpoint_config(&self, host: &str) -> EndpointConfig {
        if host.eq_ignore_ascii_case(DEFAULT_HOST) {
            EndpointConfig {
                api_base_url: self.github_api_origin.clone(),
                graphql_url: format!("{}/graphql", self.github_api_origin),
            }
        } else {
            let trimmed_host = host.trim();
            EndpointConfig {
                api_base_url: format!("https://{trimmed_host}/api/v3"),
                graphql_url: format!("https://{trimmed_host}/api/graphql"),
            }
        }
    }
}

#[derive(Clone)]
pub struct AuthService {
    db: Arc<Db>,
    token_store: Arc<dyn TokenStore>,
    gh_cli: Arc<dyn GhCli>,
    http_client: reqwest::Client,
    config: AuthConfig,
    endpoint_overrides: Arc<HashMap<String, EndpointOverride>>,
    endpoint_cache: Arc<std::sync::RwLock<HashMap<String, EndpointConfig>>>,
}

impl AuthService {
    pub fn new(db: Arc<Db>) -> Result<Self, AuthError> {
        let http_client = reqwest::Client::builder()
            .user_agent("pr-cockpit/0.1")
            .build()?;
        let config = AuthConfig::default();
        let endpoint_overrides = Arc::new(load_endpoint_overrides(
            config.endpoint_overrides_path.as_ref(),
        ));
        Ok(Self {
            db,
            token_store: Arc::new(KeyringTokenStore::default()),
            gh_cli: Arc::new(SystemGhCli),
            http_client,
            config,
            endpoint_overrides,
            endpoint_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
        })
    }

    pub fn with_dependencies(
        db: Arc<Db>,
        token_store: Arc<dyn TokenStore>,
        gh_cli: Arc<dyn GhCli>,
        http_client: reqwest::Client,
        config: AuthConfig,
    ) -> Self {
        let endpoint_overrides = Arc::new(load_endpoint_overrides(
            config.endpoint_overrides_path.as_ref(),
        ));
        Self {
            db,
            token_store,
            gh_cli,
            http_client,
            config,
            endpoint_overrides,
            endpoint_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn db(&self) -> Arc<Db> {
        Arc::clone(&self.db)
    }

    pub fn token_store(&self) -> Arc<dyn TokenStore> {
        Arc::clone(&self.token_store)
    }

    pub fn endpoint_config_for_host(&self, host: &str) -> EndpointConfig {
        let key = normalize_host(host);
        if let Ok(cache) = self.endpoint_cache.read() {
            if let Some(cached) = cache.get(&key) {
                return cached.clone();
            }
        }

        let mut resolved = self.config.default_endpoint_config(&key);
        if let Some(endpoint_override) = self.endpoint_overrides.get(&key) {
            resolved = EndpointConfig {
                api_base_url: normalize_endpoint_url(&endpoint_override.api_base_url),
                graphql_url: normalize_endpoint_url(&endpoint_override.graphql_url),
            };
        }

        if let Ok(mut cache) = self.endpoint_cache.write() {
            cache.insert(key, resolved.clone());
        }
        resolved
    }

    pub async fn detect_gh_token(
        &self,
        accept_import: bool,
    ) -> Result<GhDetectionResult, AuthError> {
        let detected = gh::detect_gh_scopes(self.gh_cli.as_ref())?;
        if !detected.host.eq_ignore_ascii_case(DEFAULT_HOST) {
            return Err(AuthError::UnsupportedHost);
        }
        if accept_import {
            let secret = StoredTokenSecret {
                access_token: detected.token,
                refresh_token: None,
                refresh_token_expires_in: None,
            };
            let _ = self
                .save_token_record(
                    &detected.host,
                    &detected.login,
                    "gh-cli",
                    &detected.scopes,
                    &secret,
                )
                .await?;
        }

        Ok(GhDetectionResult {
            host: detected.host,
            login: detected.login,
            scopes: detected.scopes,
            source: "gh".to_string(),
            saved: accept_import,
        })
    }

    pub async fn oauth_device_start(
        &self,
        input: OAuthDeviceStartInput,
    ) -> Result<OAuthDeviceStart, AuthError> {
        let host = input
            .host
            .unwrap_or_else(|| self.config.default_host.clone());
        if !host.eq_ignore_ascii_case(DEFAULT_HOST) {
            return Err(AuthError::UnsupportedHost);
        }
        let scope_str = normalize_scopes_joined(&input.scopes);
        let endpoint = format!("{}/login/device/code", self.config.web_origin(&host));
        let response = self
            .http_client
            .post(endpoint)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.config.oauth_client_id.as_str()),
                ("scope", scope_str.as_str()),
            ])
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AuthError::HttpStatus(response.status()));
        }
        let payload: OAuthDeviceStartPayload = response.json().await?;
        Ok(OAuthDeviceStart {
            host,
            device_code: payload.device_code,
            user_code: payload.user_code,
            verification_uri: payload.verification_uri,
            expires_in_seconds: payload.expires_in,
            interval_seconds: payload.interval.unwrap_or(5),
        })
    }

    pub async fn oauth_device_poll(
        &self,
        input: OAuthDevicePollInput,
    ) -> Result<OAuthDevicePollResult, AuthError> {
        let host = input
            .host
            .unwrap_or_else(|| self.config.default_host.clone());
        if !host.eq_ignore_ascii_case(DEFAULT_HOST) {
            return Err(AuthError::UnsupportedHost);
        }
        let endpoint = format!("{}/login/oauth/access_token", self.config.web_origin(&host));
        let response = self
            .http_client
            .post(endpoint)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.config.oauth_client_id.as_str()),
                ("device_code", input.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AuthError::HttpStatus(response.status()));
        }
        let payload: OAuthAccessTokenPayload = response.json().await?;
        if let Some(error_code) = payload.error.as_deref() {
            return Ok(match error_code {
                "authorization_pending" => OAuthDevicePollResult::Pending {
                    interval_seconds: input.interval_seconds.max(1),
                },
                "slow_down" => OAuthDevicePollResult::SlowDown {
                    interval_seconds: input.interval_seconds.saturating_add(5).max(1),
                },
                "access_denied" => OAuthDevicePollResult::Denied,
                "expired_token" => OAuthDevicePollResult::Expired,
                _ => return Err(AuthError::OAuthFlowFailed),
            });
        }

        let access_token = payload.access_token.ok_or(AuthError::OAuthFlowFailed)?;
        let validated = self.validate_token(&host, &access_token).await?;
        let scopes = if let Some(scope) = payload.scope {
            normalize_scopes(&scope)
        } else {
            validated.scopes.clone()
        };
        let secret = StoredTokenSecret {
            access_token,
            refresh_token: payload.refresh_token,
            refresh_token_expires_in: payload.refresh_token_expires_in,
        };
        let account = self
            .save_token_record(&host, &validated.login, "oauth-device", &scopes, &secret)
            .await?;
        Ok(OAuthDevicePollResult::Authorized { account })
    }

    pub async fn pat_save(&self, input: PatSaveInput) -> Result<AuthAccount, AuthError> {
        let host = input
            .host
            .unwrap_or_else(|| self.config.default_host.clone());
        let validated = self.validate_token(&host, &input.token).await?;
        let secret = StoredTokenSecret {
            access_token: input.token,
            refresh_token: None,
            refresh_token_expires_in: None,
        };
        self.save_token_record(&host, &validated.login, "pat", &validated.scopes, &secret)
            .await
    }

    pub async fn list_accounts(&self) -> Result<AccountsListResponse, AuthError> {
        let rows = self.db.list_auth_accounts().await?;
        let active_id = self.db.active_account_id().await?;
        let active = rows
            .iter()
            .find(|row| active_id.as_deref() == Some(row.id.as_str()))
            .map(to_locator);
        let accounts = rows
            .into_iter()
            .map(|row| {
                to_auth_account(
                    &row,
                    active_id.as_deref() == Some(row.id.as_str()),
                    self.endpoint_config_for_host(&row.host),
                )
            })
            .collect();
        Ok(AccountsListResponse { active, accounts })
    }

    pub async fn switch_account(&self, locator: AccountLocator) -> Result<AuthAccount, AuthError> {
        let account = self
            .db
            .find_auth_account(&locator.host, &locator.login)
            .await?
            .ok_or(AuthError::AccountNotFound)?;
        self.db
            .set_active_account_id(&account.id, now_epoch_seconds()?)
            .await?;
        let _ = self.endpoint_config_for_host(&account.host);
        Ok(to_auth_account(
            &account,
            true,
            self.endpoint_config_for_host(&account.host),
        ))
    }

    pub async fn remove_account(&self, locator: AccountLocator) -> Result<(), AuthError> {
        let removed = self
            .db
            .remove_auth_account(&locator.host, &locator.login, now_epoch_seconds()?)
            .await?;
        let Some(account) = removed else {
            return Err(AuthError::AccountNotFound);
        };
        self.token_store.delete(&account.host, &account.login)?;
        Ok(())
    }

    pub async fn auth_refresh(&self, locator: &AccountLocator) -> Result<bool, AuthError> {
        let account = self
            .db
            .find_auth_account(&locator.host, &locator.login)
            .await?
            .ok_or(AuthError::AccountNotFound)?;
        if account.token_kind != "oauth-device" {
            return Ok(false);
        }
        let Some(secret) = self.token_store.get(&locator.host, &locator.login)? else {
            return Ok(false);
        };
        let Some(refresh_token) = secret.refresh_token else {
            return Ok(false);
        };

        let endpoint = format!(
            "{}/login/oauth/access_token",
            self.config.web_origin(&locator.host)
        );
        let response = self
            .http_client
            .post(endpoint)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.config.oauth_client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let payload: OAuthAccessTokenPayload = response.json().await?;
        let Some(access_token) = payload.access_token else {
            return Ok(false);
        };
        let replacement = StoredTokenSecret {
            access_token,
            refresh_token: payload.refresh_token.or(Some(refresh_token)),
            refresh_token_expires_in: payload.refresh_token_expires_in,
        };
        self.token_store
            .put(&locator.host, &locator.login, &replacement)?;
        if let Some(scope_str) = payload.scope {
            let scopes = normalize_scopes(&scope_str);
            self.db
                .upsert_auth_account(
                    &locator.host,
                    &locator.login,
                    "oauth-device",
                    &normalize_scopes_joined(&scopes),
                    now_epoch_seconds()?,
                )
                .await?;
        }
        Ok(true)
    }

    pub async fn token_for_account(
        &self,
        locator: &AccountLocator,
    ) -> Result<(AuthAccountRow, StoredTokenSecret), AuthError> {
        let account = self
            .db
            .find_auth_account(&locator.host, &locator.login)
            .await?
            .ok_or(AuthError::AccountNotFound)?;
        let secret = self
            .token_store
            .get(&locator.host, &locator.login)?
            .ok_or(AuthError::InvalidToken)?;
        Ok((account, secret))
    }

    pub async fn account_secret_by_id(
        &self,
        account_id: &str,
    ) -> Result<(AuthAccountRow, StoredTokenSecret), AuthError> {
        let account = self
            .db
            .auth_account_by_id(account_id)
            .await?
            .ok_or(AuthError::AccountNotFound)?;
        let secret = self
            .token_store
            .get(&account.host, &account.login)?
            .ok_or(AuthError::InvalidToken)?;
        Ok((account, secret))
    }

    pub async fn latest_locator_for_host(&self, host: &str) -> Result<AccountLocator, AuthError> {
        let normalized = normalize_host(host);
        let mut rows = self.db.list_auth_accounts().await?;
        rows.sort_by_key(|row| std::cmp::Reverse(row.updated_at));
        let account = rows
            .into_iter()
            .find(|row| normalize_host(&row.host) == normalized)
            .ok_or(AuthError::AccountNotFound)?;
        Ok(AccountLocator {
            host: account.host,
            login: account.login,
        })
    }

    async fn save_token_record(
        &self,
        host: &str,
        login: &str,
        token_kind: &str,
        scopes: &[String],
        secret: &StoredTokenSecret,
    ) -> Result<AuthAccount, AuthError> {
        self.token_store.put(host, login, secret)?;
        let now = now_epoch_seconds()?;
        let scopes_joined = normalize_scopes_joined(scopes);
        let account = match self
            .db
            .upsert_auth_account(host, login, token_kind, &scopes_joined, now)
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = self.token_store.delete(host, login);
                return Err(AuthError::Db(error));
            }
        };
        let active_id = self.db.active_account_id().await?;
        Ok(to_auth_account(
            &account,
            active_id.as_deref() == Some(account.id.as_str()),
            self.endpoint_config_for_host(host),
        ))
    }

    async fn validate_token(&self, host: &str, token: &str) -> Result<ValidatedToken, AuthError> {
        let endpoints = self.endpoint_config_for_host(host);
        let payload = token_client::resolve_login_for_api_base_with_client(
            &self.http_client,
            &endpoints.api_base_url,
            token,
        )
        .await?;
        Ok(ValidatedToken {
            login: payload.login,
            scopes: payload.scopes,
        })
    }
}

#[derive(Debug, Deserialize)]
struct OAuthDeviceStartPayload {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OAuthAccessTokenPayload {
    access_token: Option<String>,
    scope: Option<String>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
    error: Option<String>,
}

#[derive(Debug)]
struct ValidatedToken {
    login: String,
    scopes: Vec<String>,
}

fn to_auth_account(
    row: &AuthAccountRow,
    is_active: bool,
    endpoints: EndpointConfig,
) -> AuthAccount {
    AuthAccount {
        host: row.host.clone(),
        login: row.login.clone(),
        api_base_url: endpoints.api_base_url,
        graphql_url: endpoints.graphql_url,
        token_kind: row.token_kind.clone(),
        scopes: normalize_scopes(&row.scopes),
        created_at: row.created_at,
        updated_at: row.updated_at,
        is_active,
    }
}

fn to_locator(row: &AuthAccountRow) -> AccountLocator {
    AccountLocator {
        host: row.host.clone(),
        login: row.login.clone(),
    }
}

fn now_epoch_seconds() -> Result<i64, AuthError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AuthError::Time)?;
    i64::try_from(elapsed.as_secs()).map_err(|_| AuthError::Time)
}

pub fn normalize_scopes(raw: &str) -> Vec<String> {
    raw.replace('\'', "")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_scopes_joined(scopes: &[String]) -> String {
    scopes.join(",")
}

pub fn derive_endpoint_config(host: &str) -> EndpointConfig {
    let config = AuthConfig::default();
    let normalized = normalize_host(host);
    let overrides = load_endpoint_overrides(config.endpoint_overrides_path.as_ref());
    if let Some(endpoint_override) = overrides.get(&normalized) {
        return EndpointConfig {
            api_base_url: normalize_endpoint_url(&endpoint_override.api_base_url),
            graphql_url: normalize_endpoint_url(&endpoint_override.graphql_url),
        };
    }
    config.default_endpoint_config(&normalized)
}

fn normalize_host(host: &str) -> String {
    host.trim().to_ascii_lowercase()
}

fn normalize_endpoint_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

fn default_endpoint_overrides_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PR_COCKPIT_HOSTS_TOML") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|base| base.join("pr-cockpit").join("hosts.toml"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/pr-cockpit/hosts.toml"))
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(xdg).join("pr-cockpit").join("hosts.toml"));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".config").join("pr-cockpit").join("hosts.toml"))
    }
}

#[derive(Debug, Deserialize, Default)]
struct HostOverridesFile {
    #[serde(default)]
    hosts: HashMap<String, EndpointOverride>,
}

fn load_endpoint_overrides(path: Option<&PathBuf>) -> HashMap<String, EndpointOverride> {
    let Some(path) = path else {
        return HashMap::new();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(parsed) = toml::from_str::<HostOverridesFile>(&contents) else {
        tracing::warn!(
            target: "auth",
            path = %path.display(),
            "failed to parse endpoint override config"
        );
        return HashMap::new();
    };

    parsed
        .hosts
        .into_iter()
        .map(|(host, endpoint_override)| {
            (
                normalize_host(&host),
                EndpointOverride {
                    api_base_url: normalize_endpoint_url(&endpoint_override.api_base_url),
                    graphql_url: normalize_endpoint_url(&endpoint_override.graphql_url),
                },
            )
        })
        .collect()
}

pub async fn auth_detect_gh_token_impl(
    auth: &AuthService,
    accept_import: bool,
) -> Result<GhDetectionResult, AuthCommandError> {
    auth.detect_gh_token(accept_import)
        .await
        .map_err(AuthCommandError::from)
}

pub async fn auth_oauth_device_start_impl(
    auth: &AuthService,
    input: OAuthDeviceStartInput,
) -> Result<OAuthDeviceStart, AuthCommandError> {
    auth.oauth_device_start(input)
        .await
        .map_err(AuthCommandError::from)
}

pub async fn auth_oauth_device_poll_impl(
    auth: &AuthService,
    input: OAuthDevicePollInput,
) -> Result<OAuthDevicePollResult, AuthCommandError> {
    auth.oauth_device_poll(input)
        .await
        .map_err(AuthCommandError::from)
}

pub async fn auth_pat_save_impl(
    auth: &AuthService,
    input: PatSaveInput,
) -> Result<AuthAccount, AuthCommandError> {
    auth.pat_save(input).await.map_err(AuthCommandError::from)
}

pub async fn auth_list_accounts_impl(
    auth: &AuthService,
) -> Result<AccountsListResponse, AuthCommandError> {
    auth.list_accounts().await.map_err(AuthCommandError::from)
}

pub async fn auth_switch_account_impl(
    auth: &AuthService,
    locator: AccountLocator,
) -> Result<AuthAccount, AuthCommandError> {
    auth.switch_account(locator)
        .await
        .map_err(AuthCommandError::from)
}

pub async fn auth_remove_account_impl(
    auth: &AuthService,
    locator: AccountLocator,
) -> Result<(), AuthCommandError> {
    auth.remove_account(locator)
        .await
        .map_err(AuthCommandError::from)
}

pub async fn auth_refresh_impl(
    auth: &AuthService,
    locator: AccountLocator,
) -> Result<RefreshResult, AuthCommandError> {
    let refreshed = auth
        .auth_refresh(&locator)
        .await
        .map_err(AuthCommandError::from)?;
    Ok(RefreshResult { refreshed })
}

#[tauri::command]
#[specta::specta]
pub async fn auth_detect_gh_token(
    auth: tauri::State<'_, Arc<AuthService>>,
    accept_import: bool,
) -> Result<GhDetectionResult, AuthCommandError> {
    auth_detect_gh_token_impl(auth.inner(), accept_import).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_oauth_device_start(
    auth: tauri::State<'_, Arc<AuthService>>,
    input: OAuthDeviceStartInput,
) -> Result<OAuthDeviceStart, AuthCommandError> {
    auth_oauth_device_start_impl(auth.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_oauth_device_poll(
    auth: tauri::State<'_, Arc<AuthService>>,
    input: OAuthDevicePollInput,
) -> Result<OAuthDevicePollResult, AuthCommandError> {
    auth_oauth_device_poll_impl(auth.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_pat_save(
    auth: tauri::State<'_, Arc<AuthService>>,
    input: PatSaveInput,
) -> Result<AuthAccount, AuthCommandError> {
    auth_pat_save_impl(auth.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_list_accounts(
    auth: tauri::State<'_, Arc<AuthService>>,
) -> Result<AccountsListResponse, AuthCommandError> {
    auth_list_accounts_impl(auth.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_switch_account(
    auth: tauri::State<'_, Arc<AuthService>>,
    locator: AccountLocator,
) -> Result<AuthAccount, AuthCommandError> {
    auth_switch_account_impl(auth.inner(), locator).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_remove_account(
    auth: tauri::State<'_, Arc<AuthService>>,
    locator: AccountLocator,
) -> Result<(), AuthCommandError> {
    auth_remove_account_impl(auth.inner(), locator).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_refresh(
    auth: tauri::State<'_, Arc<AuthService>>,
    locator: AccountLocator,
) -> Result<RefreshResult, AuthCommandError> {
    auth_refresh_impl(auth.inner(), locator).await
}

pub fn command_names() -> &'static [&'static str] {
    &[
        "auth_detect_gh_token",
        "auth_oauth_device_start",
        "auth_oauth_device_poll",
        "auth_pat_save",
        "auth_list_accounts",
        "auth_switch_account",
        "auth_remove_account",
        "auth_refresh",
    ]
}
