use std::sync::Arc;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};

use serde::Deserialize;

use super::{derive_endpoint_config, normalize_scopes, AccountLocator, AuthError, AuthService};

#[derive(Clone)]
pub struct TokenClient {
    auth_service: Arc<AuthService>,
    http_client: reqwest::Client,
}

impl TokenClient {
    pub fn new(auth_service: Arc<AuthService>, http_client: reqwest::Client) -> Self {
        Self {
            auth_service,
            http_client,
        }
    }

    pub fn from_auth_service(auth_service: Arc<AuthService>) -> Self {
        Self {
            auth_service,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn request(
        &self,
        locator: &AccountLocator,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::Response, AuthError> {
        self.request_with(locator, method, url, HeaderMap::new(), None)
            .await
    }

    pub async fn request_with(
        &self,
        locator: &AccountLocator,
        method: reqwest::Method,
        url: &str,
        extra_headers: HeaderMap,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, AuthError> {
        let (account, secret) = self.auth_service.token_for_account(locator).await?;
        let mut response = self
            .dispatch(
                &method,
                url,
                &secret.access_token,
                &extra_headers,
                body.clone(),
            )
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            && account.token_kind == "oauth-device"
            && self.auth_service.auth_refresh(locator).await?
        {
            let (_, rotated_secret) = self.auth_service.token_for_account(locator).await?;
            response = self
                .dispatch(
                    &method,
                    url,
                    &rotated_secret.access_token,
                    &extra_headers,
                    body,
                )
                .await?;
        }
        Ok(response)
    }

    async fn dispatch(
        &self,
        method: &reqwest::Method,
        url: &str,
        token: &str,
        extra_headers: &HeaderMap,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::Response, AuthError> {
        let mut headers = auth_headers(token)?;
        headers.extend(extra_headers.clone());
        let mut request = self
            .http_client
            .request(method.clone(), url)
            .headers(headers);
        if let Some(bytes) = body {
            request = request.body(bytes);
        }
        request.send().await.map_err(AuthError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLogin {
    pub login: String,
    pub scopes: Vec<String>,
}

pub async fn resolve_login_for_host(host: &str, token: &str) -> Result<ResolvedLogin, AuthError> {
    let client = reqwest::Client::new();
    resolve_login_for_host_with_client(&client, host, token).await
}

pub async fn resolve_login_for_host_with_client(
    http_client: &reqwest::Client,
    host: &str,
    token: &str,
) -> Result<ResolvedLogin, AuthError> {
    let endpoints = derive_endpoint_config(host);
    resolve_login_for_api_base_with_client(http_client, &endpoints.api_base_url, token).await
}

pub async fn resolve_login_for_api_base_with_client(
    http_client: &reqwest::Client,
    api_base_url: &str,
    token: &str,
) -> Result<ResolvedLogin, AuthError> {
    let endpoint = format!("{}/user", api_base_url.trim_end_matches('/'));
    let response = http_client
        .get(endpoint)
        .headers(auth_headers(token)?)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AuthError::InvalidToken);
    }
    if !response.status().is_success() {
        return Err(AuthError::HttpStatus(response.status()));
    }
    let scopes = response
        .headers()
        .get("x-oauth-scopes")
        .and_then(|value| value.to_str().ok())
        .map(normalize_scopes)
        .unwrap_or_default();
    let user: UserResponse = response.json().await?;
    Ok(ResolvedLogin {
        login: user.login,
        scopes,
    })
}

fn auth_headers(token: &str) -> Result<HeaderMap, AuthError> {
    let mut auth_value = HeaderValue::from_str(&format!("Bearer {token}"))?;
    auth_value.set_sensitive(true);

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, auth_value);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static("pr-cockpit/0.1"));
    Ok(headers)
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    login: String,
}
