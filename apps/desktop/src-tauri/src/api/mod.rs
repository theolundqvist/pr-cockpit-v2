use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH,
    LAST_MODIFIED,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::token_client::TokenClient;
use crate::auth::{derive_endpoint_config, AccountLocator};
use crate::db::{Db, PrPatchRecord, SyncCursorUpdate};

pub mod check_logs;

pub const PR_DETAIL_QUERY: &str = include_str!("queries/PrDetail.graphql");
pub const INBOX_REFRESH_QUERY: &str = include_str!("queries/InboxRefresh.graphql");
pub const ADD_PULL_REQUEST_REVIEW_THREAD_REPLY_MUTATION: &str =
    include_str!("queries/mutations/addPullRequestReviewThreadReply.graphql");
pub const SUBMIT_PULL_REQUEST_REVIEW_MUTATION: &str =
    include_str!("queries/mutations/submitPullRequestReview.graphql");
pub const RESOLVE_REVIEW_THREAD_MUTATION: &str =
    include_str!("queries/mutations/resolveReviewThread.graphql");
pub const UNRESOLVE_REVIEW_THREAD_MUTATION: &str =
    include_str!("queries/mutations/unresolveReviewThread.graphql");
pub const ADD_PROJECT_V2_ITEM_MUTATION: &str =
    include_str!("queries/mutations/addProjectV2ItemById.graphql");
pub const UPDATE_PROJECT_V2_ITEM_FIELD_VALUE_MUTATION: &str =
    include_str!("queries/mutations/updateProjectV2ItemFieldValue.graphql");
pub const CONVERT_PULL_REQUEST_TO_DRAFT_MUTATION: &str =
    include_str!("queries/mutations/convertPullRequestToDraft.graphql");
pub const MARK_PULL_REQUEST_READY_FOR_REVIEW_MUTATION: &str =
    include_str!("queries/mutations/markPullRequestReadyForReview.graphql");
pub const ENABLE_PULL_REQUEST_AUTOMERGE_MUTATION: &str =
    include_str!("queries/mutations/enablePullRequestAutoMerge.graphql");
pub const DISABLE_PULL_REQUEST_AUTOMERGE_MUTATION: &str =
    include_str!("queries/mutations/disablePullRequestAutoMerge.graphql");
pub const ENQUEUE_PULL_REQUEST_MUTATION: &str =
    include_str!("queries/mutations/enqueuePullRequest.graphql");
pub const DEQUEUE_PULL_REQUEST_MUTATION: &str =
    include_str!("queries/mutations/dequeuePullRequest.graphql");
pub const REORDER_MERGE_QUEUE_ENTRY_MUTATION: &str =
    include_str!("queries/mutations/reorderMergeQueueEntry.graphql");
pub const RERUN_CHECK_SUITE_MUTATION: &str =
    include_str!("queries/mutations/rerunCheckSuite.graphql");
pub const PR_DETAIL_QUERY_REVISION: &str = "2026-05-18.m5.v1";
pub const INBOX_REFRESH_QUERY_REVISION: &str = "2026-05-17.m1.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiResource {
    Graphql,
    Core,
}

impl ApiResource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Graphql => "graphql",
            Self::Core => "core",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitSnapshot {
    pub remaining: i64,
    pub limit_total: i64,
    pub used: Option<i64>,
    pub reset_at_epoch: i64,
}

#[derive(Debug, Clone)]
pub struct ResponseMetadata {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub poll_interval_seconds: Option<u64>,
    pub rate_limit: Option<RateLimitSnapshot>,
}

#[derive(Debug)]
pub enum ConditionalResponse<T> {
    NotModified(ResponseMetadata),
    Modified {
        payload: T,
        metadata: ResponseMetadata,
    },
}

#[derive(Debug, Clone)]
pub struct GithubApiConfig {
    pub api_origin: String,
    pub graphql_origin: String,
}

impl Default for GithubApiConfig {
    fn default() -> Self {
        Self {
            api_origin: "https://api.github.com".to_string(),
            graphql_origin: "https://api.github.com/graphql".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct GithubClient {
    token_client: TokenClient,
    db: Arc<Db>,
    resolver: Arc<dyn AccountResolver>,
    probe_url: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedAccountEndpoints {
    pub locator: AccountLocator,
    pub token: String,
    pub api_base_url: String,
    pub graphql_url: String,
}

#[async_trait]
pub trait AccountResolver: Send + Sync {
    async fn resolve(&self, account_id: &str) -> Result<ResolvedAccountEndpoints>;
}

#[derive(Clone)]
struct DbAccountResolver {
    db: Arc<Db>,
    config: GithubApiConfig,
}

#[async_trait]
impl AccountResolver for DbAccountResolver {
    async fn resolve(&self, account_id: &str) -> Result<ResolvedAccountEndpoints> {
        let account = self
            .db
            .auth_account_by_id(account_id)
            .await?
            .ok_or_else(|| anyhow!("account not found for id `{account_id}`"))?;
        let endpoint = if account.host.eq_ignore_ascii_case("github.com") {
            (
                self.config.api_origin.clone(),
                self.config.graphql_origin.clone(),
            )
        } else {
            let derived = derive_endpoint_config(&account.host);
            (derived.api_base_url, derived.graphql_url)
        };
        Ok(ResolvedAccountEndpoints {
            locator: AccountLocator {
                host: account.host,
                login: account.login,
            },
            token: String::new(),
            api_base_url: endpoint.0,
            graphql_url: endpoint.1,
        })
    }
}

pub struct PullDiffRequest<'a> {
    pub account_id: &'a str,
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: i64,
    pub pr_id: &'a str,
    pub head_sha: &'a str,
}

impl GithubClient {
    pub fn new(token_client: TokenClient, db: Arc<Db>) -> Self {
        let config = GithubApiConfig::default();
        let probe_url = config.api_origin.clone();
        Self {
            token_client,
            db: Arc::clone(&db),
            resolver: Arc::new(DbAccountResolver {
                db: Arc::clone(&db),
                config,
            }),
            probe_url,
        }
    }

    pub fn with_config(token_client: TokenClient, db: Arc<Db>, config: GithubApiConfig) -> Self {
        let probe_url = config.api_origin.clone();
        Self {
            token_client,
            db: Arc::clone(&db),
            resolver: Arc::new(DbAccountResolver {
                db: Arc::clone(&db),
                config,
            }),
            probe_url,
        }
    }

    pub fn with_account_resolver(
        token_client: TokenClient,
        db: Arc<Db>,
        resolver: Arc<dyn AccountResolver>,
        probe_url: String,
    ) -> Self {
        Self {
            token_client,
            db,
            resolver,
            probe_url,
        }
    }

    pub fn probe_url(&self) -> String {
        self.probe_url.clone()
    }

    pub async fn resolve_account_endpoints(
        &self,
        account_id: &str,
    ) -> Result<ResolvedAccountEndpoints> {
        self.resolve_account(account_id).await
    }

    pub async fn graphql<T: DeserializeOwned>(
        &self,
        account_id: &str,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<(T, Option<RateLimitSnapshot>)> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let body = serde_json::to_vec(&json!({
            "query": query,
            "variables": variables
        }))?;
        let response = self
            .token_client
            .request_with(
                &resolved.locator,
                reqwest::Method::POST,
                &resolved.graphql_url,
                headers,
                Some(body),
            )
            .await?;
        let rate_limit = parse_rate_limit_headers(response.headers());
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("graphql request failed ({status}): {body}"));
        }

        let envelope: GraphqlEnvelope<T> = response.json().await?;
        if let Some(errors) = envelope.errors {
            return Err(anyhow!(
                "graphql errors: {}",
                serde_json::to_string(&errors)?
            ));
        }
        let data = envelope
            .data
            .ok_or_else(|| anyhow!("graphql response missing data"))?;
        Ok((data, rate_limit))
    }

    pub async fn graphql_mutation<T: DeserializeOwned>(
        &self,
        account_id: &str,
        query: &str,
        variables: serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<(T, Option<RateLimitSnapshot>)> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(idempotency_key) = idempotency_key {
            headers.insert(
                "Idempotency-Key",
                HeaderValue::from_str(idempotency_key).context("invalid idempotency key header")?,
            );
        }

        let body = serde_json::to_vec(&json!({
            "query": query,
            "variables": variables
        }))?;
        let response = self
            .token_client
            .request_with(
                &resolved.locator,
                reqwest::Method::POST,
                &resolved.graphql_url,
                headers,
                Some(body),
            )
            .await?;
        let rate_limit = parse_rate_limit_headers(response.headers());
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("graphql mutation failed ({status}): {body}"));
        }

        let envelope: GraphqlEnvelope<T> = response.json().await?;
        if let Some(errors) = envelope.errors {
            return Err(anyhow!(
                "graphql mutation errors: {}",
                serde_json::to_string(&errors)?
            ));
        }
        let data = envelope
            .data
            .ok_or_else(|| anyhow!("graphql mutation response missing data"))?;
        Ok((data, rate_limit))
    }

    pub async fn rest_mutation_json<T: DeserializeOwned>(
        &self,
        account_id: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        idempotency_key: Option<&str>,
    ) -> Result<(Option<T>, Option<RateLimitSnapshot>)> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        if body.is_some() {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        if let Some(idempotency_key) = idempotency_key {
            headers.insert(
                "Idempotency-Key",
                HeaderValue::from_str(idempotency_key).context("invalid idempotency key header")?,
            );
        }

        let url = format!("{}{}", resolved.api_base_url, path);
        let payload = body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .context("serializing mutation payload")?;
        let response = self
            .token_client
            .request_with(&resolved.locator, method, &url, headers, payload)
            .await?;
        let rate_limit = parse_rate_limit_headers(response.headers());

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("rest mutation failed ({status}): {body}"));
        }

        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok((None, rate_limit));
        }
        let bytes = response.bytes().await?;
        if bytes.is_empty() {
            return Ok((None, rate_limit));
        }
        let payload = serde_json::from_slice::<T>(&bytes)
            .with_context(|| format!("decoding mutation response for `{path}`"))?;
        Ok((Some(payload), rate_limit))
    }

    pub async fn rest_mutation_no_response(
        &self,
        account_id: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Option<RateLimitSnapshot>> {
        let (_payload, rate_limit) = self
            .rest_mutation_json::<serde_json::Value>(
                account_id,
                method,
                path,
                body,
                idempotency_key,
            )
            .await?;
        Ok(rate_limit)
    }

    pub async fn rest_get_json<T: DeserializeOwned>(
        &self,
        account_id: &str,
        path: &str,
    ) -> Result<(T, Option<RateLimitSnapshot>)> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        let url = format!("{}{}", resolved.api_base_url, path);
        let response = self
            .token_client
            .request_with(&resolved.locator, reqwest::Method::GET, &url, headers, None)
            .await?;
        let rate_limit = parse_rate_limit_headers(response.headers());
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("rest get failed ({status}): {body}"));
        }
        let payload = response.json::<T>().await?;
        Ok((payload, rate_limit))
    }

    pub async fn rest_get_stream(
        &self,
        account_id: &str,
        path: &str,
    ) -> Result<(reqwest::Response, Option<RateLimitSnapshot>)> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        let url = format!("{}{}", resolved.api_base_url, path);
        let response = self
            .token_client
            .request_with(&resolved.locator, reqwest::Method::GET, &url, headers, None)
            .await?;
        let rate_limit = parse_rate_limit_headers(response.headers());
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("rest get stream failed ({status}): {body}"));
        }
        Ok((response, rate_limit))
    }

    pub async fn get_json_conditional<T: DeserializeOwned>(
        &self,
        account_id: &str,
        resource: &str,
        path: &str,
        query_pairs: &[(&str, String)],
    ) -> Result<ConditionalResponse<T>> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut url = reqwest::Url::parse(&format!("{}{}", resolved.api_base_url, path))
            .with_context(|| format!("building github url for {path}"))?;
        for (key, value) in query_pairs {
            url.query_pairs_mut().append_pair(key, value);
        }

        let existing_cursor = self.db.sync_cursor(account_id, resource).await?;
        let existing_etag = existing_cursor
            .as_ref()
            .and_then(|cursor| cursor.etag.clone());
        let mut headers = HeaderMap::new();
        if let Some(etag) = &existing_etag {
            headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).context("invalid etag value")?,
            );
        }

        let response = self
            .token_client
            .request_with(
                &resolved.locator,
                reqwest::Method::GET,
                url.as_str(),
                headers,
                None,
            )
            .await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(ConditionalResponse::NotModified(parse_response_metadata(
                response.headers(),
                false,
            )));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("rest request failed ({status}): {body}"));
        }

        let metadata = parse_response_metadata(response.headers(), true);
        let payload = response.json::<T>().await?;
        self.db
            .update_sync_cursor(&SyncCursorUpdate {
                account_id: account_id.to_string(),
                resource: resource.to_string(),
                cursor: existing_cursor.and_then(|row| row.cursor),
                etag: metadata.etag.clone(),
                expected_previous_etag: existing_etag,
                fetched_at: now_epoch_seconds()?,
            })
            .await?;

        Ok(ConditionalResponse::Modified { payload, metadata })
    }

    pub async fn fetch_pull_diff(
        &self,
        request: PullDiffRequest<'_>,
    ) -> Result<ConditionalResponse<DiffFetchResult>> {
        let resolved = self
            .resolve_account(request.account_id)
            .await
            .with_context(|| format!("resolving account `{}`", request.account_id))?;
        let resource = format!(
            "pull-diff:{}/{}/{}",
            request.owner, request.repo, request.number
        );
        let existing_cursor = self.db.sync_cursor(request.account_id, &resource).await?;
        let existing_etag = existing_cursor
            .as_ref()
            .and_then(|cursor| cursor.etag.clone());

        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3.diff"),
        );
        if let Some(etag) = &existing_etag {
            headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).context("invalid etag value")?,
            );
        }

        let path = format!(
            "{}/repos/{}/{}/pulls/{}",
            resolved.api_base_url, request.owner, request.repo, request.number
        );
        let response = self
            .token_client
            .request_with(
                &resolved.locator,
                reqwest::Method::GET,
                &path,
                headers,
                None,
            )
            .await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(ConditionalResponse::NotModified(parse_response_metadata(
                response.headers(),
                false,
            )));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("pull diff request failed ({status}): {body}"));
        }

        let metadata = parse_response_metadata(response.headers(), true);
        let patch = response.text().await?;
        let patch_blob_sha = self.db.blob_store().put_patch(patch.as_bytes()).await?;
        let fetched_at = now_epoch_seconds()?;
        self.db
            .upsert_pr_patch(&PrPatchRecord {
                account_id: request.account_id.to_string(),
                pr_id: request.pr_id.to_string(),
                head_sha: request.head_sha.to_string(),
                patch_blob_sha: patch_blob_sha.clone(),
                fetched_at,
            })
            .await?;
        self.db
            .update_sync_cursor(&SyncCursorUpdate {
                account_id: request.account_id.to_string(),
                resource,
                cursor: existing_cursor.and_then(|row| row.cursor),
                etag: metadata.etag.clone(),
                expected_previous_etag: existing_etag,
                fetched_at,
            })
            .await?;

        Ok(ConditionalResponse::Modified {
            payload: DiffFetchResult { patch_blob_sha },
            metadata,
        })
    }

    pub async fn poll_notifications(
        &self,
        account_id: &str,
        participating: bool,
    ) -> Result<ConditionalResponse<Vec<GithubNotification>>> {
        let resource = "notifications";
        let existing_cursor = self.db.sync_cursor(account_id, resource).await?;
        let last_modified = existing_cursor
            .as_ref()
            .and_then(|cursor| cursor.cursor.clone());
        let mut headers = HeaderMap::new();
        if let Some(last_modified) = &last_modified {
            headers.insert(
                IF_MODIFIED_SINCE,
                HeaderValue::from_str(last_modified).context("invalid last-modified header")?,
            );
        }

        let since = last_modified.clone().unwrap_or_default();
        let response = self
            .get_json_with_headers::<Vec<GithubNotification>>(
                account_id,
                "/notifications",
                &[
                    ("since", since),
                    ("participating", participating.to_string()),
                ],
                headers,
            )
            .await?;

        match response {
            RawResponse::NotModified { metadata } => Ok(ConditionalResponse::NotModified(metadata)),
            RawResponse::Modified { payload, metadata } => {
                let cursor_value = metadata
                    .last_modified
                    .clone()
                    .or(last_modified)
                    .unwrap_or_default();
                self.db
                    .update_sync_cursor(&SyncCursorUpdate {
                        account_id: account_id.to_string(),
                        resource: resource.to_string(),
                        cursor: Some(cursor_value),
                        etag: None,
                        expected_previous_etag: None,
                        fetched_at: now_epoch_seconds()?,
                    })
                    .await?;
                Ok(ConditionalResponse::Modified { payload, metadata })
            }
        }
    }

    async fn get_json_with_headers<T: DeserializeOwned>(
        &self,
        account_id: &str,
        path: &str,
        query_pairs: &[(&str, String)],
        headers: HeaderMap,
    ) -> Result<RawResponse<T>> {
        let resolved = self
            .resolve_account(account_id)
            .await
            .with_context(|| format!("resolving account `{account_id}`"))?;
        let mut url = reqwest::Url::parse(&format!("{}{}", resolved.api_base_url, path))
            .with_context(|| format!("building github url for {path}"))?;
        for (key, value) in query_pairs {
            if !value.is_empty() {
                url.query_pairs_mut().append_pair(key, value);
            }
        }

        let response = self
            .token_client
            .request_with(
                &resolved.locator,
                reqwest::Method::GET,
                url.as_str(),
                headers,
                None,
            )
            .await?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(RawResponse::NotModified {
                metadata: parse_response_metadata(response.headers(), false),
            });
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<body unavailable>".to_string());
            return Err(anyhow!("rest request failed ({status}): {body}"));
        }
        let metadata = parse_response_metadata(response.headers(), true);
        let payload = response.json::<T>().await?;
        Ok(RawResponse::Modified { payload, metadata })
    }

    async fn resolve_account(&self, account_id: &str) -> Result<ResolvedAccountEndpoints> {
        self.resolver.resolve(account_id).await
    }
}

#[derive(Debug)]
enum RawResponse<T> {
    NotModified {
        metadata: ResponseMetadata,
    },
    Modified {
        payload: T,
        metadata: ResponseMetadata,
    },
}

#[derive(Debug)]
struct GraphqlEnvelope<T> {
    data: Option<T>,
    errors: Option<serde_json::Value>,
}

impl<'de, T> Deserialize<'de> for GraphqlEnvelope<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Inner<T> {
            data: Option<T>,
            errors: Option<serde_json::Value>,
        }

        let inner = Inner::deserialize(deserializer)?;
        Ok(Self {
            data: inner.data,
            errors: inner.errors,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubNotification {
    pub id: String,
    pub unread: bool,
    pub reason: String,
    pub updated_at: String,
    pub last_read_at: Option<String>,
    pub subject: GithubNotificationSubject,
    pub repository: GithubNotificationRepository,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubNotificationSubject {
    pub title: String,
    #[serde(rename = "type")]
    pub subject_type: String,
    pub url: Option<String>,
    pub latest_comment_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubNotificationRepository {
    pub id: i64,
    pub name: String,
    pub owner: GithubNotificationOwner,
    pub html_url: Option<String>,
    pub description: Option<String>,
    pub private: bool,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubNotificationOwner {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFetchResult {
    pub patch_blob_sha: String,
}

fn parse_response_metadata(headers: &HeaderMap, parse_rate_limit: bool) -> ResponseMetadata {
    let poll_interval_seconds = header_as_u64(headers, "x-poll-interval");
    ResponseMetadata {
        etag: header_to_string(headers.get(ETAG)),
        last_modified: header_to_string(headers.get(LAST_MODIFIED)),
        poll_interval_seconds,
        rate_limit: if parse_rate_limit {
            parse_rate_limit_headers(headers)
        } else {
            None
        },
    }
}

fn parse_rate_limit_headers(headers: &HeaderMap) -> Option<RateLimitSnapshot> {
    let remaining = header_as_i64(headers, "x-ratelimit-remaining")?;
    let limit_total = header_as_i64(headers, "x-ratelimit-limit")?;
    let reset_at_epoch = header_as_i64(headers, "x-ratelimit-reset")?;
    let used = header_as_i64(headers, "x-ratelimit-used");
    Some(RateLimitSnapshot {
        remaining,
        limit_total,
        used,
        reset_at_epoch,
    })
}

fn header_to_string(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|entry| entry.to_str().ok())
        .map(|entry| entry.to_string())
}

fn header_as_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

fn header_as_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}
