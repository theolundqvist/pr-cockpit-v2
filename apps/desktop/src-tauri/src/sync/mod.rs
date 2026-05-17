pub mod reconcile;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};

use crate::api::{
    ApiResource, ConditionalResponse, GithubClient, GithubNotification, PullDiffRequest,
    INBOX_REFRESH_QUERY, PR_DETAIL_QUERY,
};
use crate::auth::AccountLocator;
use crate::db::{Db, RateLimitBucketUpdate};
use crate::sync::reconcile::{
    reconcile_inbox_refresh, reconcile_notifications, reconcile_pr_detail, InboxRefreshData,
    NotificationPayload, PrDetailData, RefetchTarget,
};

const THROTTLE_GRAPHQL_REMAINING: i64 = 1000;
const THROTTLE_CORE_REMAINING: i64 = 500;
const UNFOCUS_PAUSE_AFTER: Duration = Duration::from_secs(5 * 60);

pub type ActionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    Focused,
    Unfocused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    Hot,
    Warm,
    Cool,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Priority {
    Foreground,
    Background,
}

pub trait TierActions: Send + Sync {
    fn run_tier<'a>(
        &'a self,
        tier: Tier,
        priority: Priority,
    ) -> ActionFuture<'a, Vec<RefetchTarget>>;
    fn run_refetch<'a>(
        &'a self,
        targets: Vec<RefetchTarget>,
        priority: Priority,
    ) -> ActionFuture<'a, ()>;
}

pub trait Clock: Send + Sync {
    fn sleep<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

#[derive(Default)]
pub struct TokioClock;

impl Clock for TokioClock {
    fn sleep<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            tokio::time::sleep(duration).await;
        })
    }
}

#[derive(Clone)]
pub struct RateLimitBudgeter {
    command_tx: mpsc::Sender<BudgetCommand>,
}

impl RateLimitBudgeter {
    pub fn start(db: Arc<Db>) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel::<BudgetCommand>(128);
        tokio::spawn(async move {
            let mut buckets = HashMap::<ApiResource, BudgetState>::new();
            while let Some(command) = command_rx.recv().await {
                match command {
                    BudgetCommand::Record {
                        account_id,
                        resource,
                        snapshot,
                    } => {
                        buckets.insert(
                            resource,
                            BudgetState {
                                remaining: snapshot.remaining,
                                used: snapshot.used,
                                reset_at_epoch: snapshot.reset_at_epoch,
                            },
                        );
                        let _ = db
                            .update_rate_limit_bucket(&RateLimitBucketUpdate {
                                account_id,
                                resource: resource.as_str().to_string(),
                                remaining: snapshot.remaining,
                                limit_total: snapshot.limit_total,
                                reset_at: snapshot.reset_at_epoch,
                                updated_at: now_epoch_seconds().unwrap_or_default(),
                            })
                            .await;
                    }
                    BudgetCommand::Allow { priority, reply_tx } => {
                        let allow = match priority {
                            Priority::Foreground => true,
                            Priority::Background => {
                                let now_epoch = now_epoch_seconds().unwrap_or_default();
                                let graphql_remaining =
                                    buckets
                                        .get(&ApiResource::Graphql)
                                        .map_or(i64::MAX, |state| {
                                            if now_epoch >= state.reset_at_epoch {
                                                i64::MAX
                                            } else {
                                                state.remaining
                                            }
                                        });
                                let core_remaining =
                                    buckets.get(&ApiResource::Core).map_or(i64::MAX, |state| {
                                        if now_epoch >= state.reset_at_epoch {
                                            i64::MAX
                                        } else {
                                            state.remaining
                                        }
                                    });
                                let _graphql_used = buckets
                                    .get(&ApiResource::Graphql)
                                    .and_then(|state| state.used)
                                    .unwrap_or_default();
                                let _core_used = buckets
                                    .get(&ApiResource::Core)
                                    .and_then(|state| state.used)
                                    .unwrap_or_default();
                                graphql_remaining >= THROTTLE_GRAPHQL_REMAINING
                                    && core_remaining >= THROTTLE_CORE_REMAINING
                            }
                        };
                        let _ = reply_tx.send(allow);
                    }
                }
            }
        });
        Self { command_tx }
    }

    pub async fn record(
        &self,
        account_id: &str,
        resource: ApiResource,
        snapshot: crate::api::RateLimitSnapshot,
    ) -> Result<()> {
        self.command_tx
            .send(BudgetCommand::Record {
                account_id: account_id.to_string(),
                resource,
                snapshot,
            })
            .await
            .context("rate-limit budgeter channel closed")
    }

    pub async fn allow(&self, priority: Priority) -> Result<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(BudgetCommand::Allow { priority, reply_tx })
            .await
            .context("rate-limit budgeter channel closed")?;
        reply_rx.await.context("rate-limit budgeter dropped reply")
    }
}

#[derive(Debug, Clone)]
struct BudgetState {
    remaining: i64,
    used: Option<i64>,
    reset_at_epoch: i64,
}

enum BudgetCommand {
    Record {
        account_id: String,
        resource: ApiResource,
        snapshot: crate::api::RateLimitSnapshot,
    },
    Allow {
        priority: Priority,
        reply_tx: oneshot::Sender<bool>,
    },
}

pub struct SyncHandle {
    focus_tx: watch::Sender<FocusState>,
    foreground_tx: mpsc::Sender<Tier>,
    refetch_tx: mpsc::Sender<Vec<RefetchTarget>>,
    shutdown_tx: broadcast::Sender<()>,
    join_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl SyncHandle {
    pub fn focus_sender(&self) -> watch::Sender<FocusState> {
        self.focus_tx.clone()
    }

    pub async fn set_focus(&self, focus: FocusState) -> Result<()> {
        self.focus_tx
            .send(focus)
            .context("sync engine focus channel closed")
    }

    pub async fn enqueue_foreground(&self, tier: Tier) -> Result<()> {
        self.foreground_tx
            .send(tier)
            .await
            .context("sync engine foreground channel closed")
    }

    pub async fn enqueue_refetch(&self, targets: Vec<RefetchTarget>) -> Result<()> {
        self.refetch_tx
            .send(targets)
            .await
            .context("sync engine refetch channel closed")
    }
}

pub async fn start(
    actions: Arc<dyn TierActions>,
    rate_limit_budgeter: RateLimitBudgeter,
) -> Result<SyncHandle> {
    let (focus_tx, focus_rx) = watch::channel(FocusState::Focused);
    let (foreground_tx, mut foreground_rx) = mpsc::channel::<Tier>(64);
    let (refetch_tx, mut refetch_rx) = mpsc::channel::<Vec<RefetchTarget>>(128);
    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let mut join_handles = Vec::new();
    for tier in [Tier::Hot, Tier::Warm, Tier::Cool, Tier::Cold] {
        let actions = Arc::clone(&actions);
        let mut focus_rx = focus_rx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let refetch_tx = refetch_tx.clone();
        let rate_limit_budgeter = rate_limit_budgeter.clone();
        join_handles.push(tokio::spawn(async move {
            let mut unfocused_since: Option<tokio::time::Instant> = None;
            loop {
                let focus_state = *focus_rx.borrow();
                if focus_state == FocusState::Unfocused && unfocused_since.is_none() {
                    unfocused_since = Some(tokio::time::Instant::now());
                } else if focus_state == FocusState::Focused {
                    unfocused_since = None;
                }
                let cadence = cadence_for(tier, focus_state, unfocused_since);
                let Some(cadence) = cadence else {
                    tokio::select! {
                        _ = focus_rx.changed() => {}
                        _ = shutdown_rx.recv() => break,
                    }
                    continue;
                };
                tokio::select! {
                    _ = tokio::time::sleep(cadence) => {
                        let allow = rate_limit_budgeter.allow(Priority::Background).await.unwrap_or(true);
                        if !allow {
                            continue;
                        }
                        if let Ok(targets) = actions.run_tier(tier, Priority::Background).await {
                            if !targets.is_empty() {
                                let _ = refetch_tx.send(targets).await;
                            }
                        }
                    }
                    _ = focus_rx.changed() => {}
                    _ = shutdown_rx.recv() => break,
                }
            }
        }));
    }

    let actions_for_foreground = Arc::clone(&actions);
    let mut shutdown_rx = shutdown_tx.subscribe();
    let refetch_tx_clone = refetch_tx.clone();
    join_handles.push(tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_tier = foreground_rx.recv() => {
                    let Some(tier) = maybe_tier else { break; };
                    if let Ok(targets) = actions_for_foreground.run_tier(tier, Priority::Foreground).await {
                        if !targets.is_empty() {
                            let _ = refetch_tx_clone.send(targets).await;
                        }
                    }
                }
                _ = shutdown_rx.recv() => break,
            }
        }
    }));

    let actions_for_refetch = Arc::clone(&actions);
    let rate_limit_budgeter = rate_limit_budgeter.clone();
    let mut shutdown_rx = shutdown_tx.subscribe();
    join_handles.push(tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_targets = refetch_rx.recv() => {
                    let Some(targets) = maybe_targets else { break; };
                    let mut dedup = Vec::new();
                    let mut seen = HashSet::<(String, String, i64)>::new();
                    for target in targets {
                        let key = (target.owner.clone(), target.repo.clone(), target.number);
                        if seen.insert(key) {
                            dedup.push(target);
                        }
                    }
                    while !rate_limit_budgeter.allow(Priority::Background).await.unwrap_or(true) {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                    let _ = actions_for_refetch.run_refetch(dedup, Priority::Background).await;
                }
                _ = shutdown_rx.recv() => break,
            }
        }
    }));

    Ok(SyncHandle {
        focus_tx,
        foreground_tx,
        refetch_tx,
        shutdown_tx,
        join_handles,
    })
}

pub async fn shutdown(mut handle: SyncHandle) -> Result<()> {
    let _ = handle.shutdown_tx.send(());
    drop(handle.focus_tx);
    drop(handle.foreground_tx);
    drop(handle.refetch_tx);
    for join in handle.join_handles.drain(..) {
        join.await.context("joining sync task")?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct RealActions {
    inner: Arc<RealActionsInner>,
}

struct RealActionsInner {
    db: Arc<Db>,
    github: GithubClient,
    account_id: String,
    locator: AccountLocator,
    budgeter: RateLimitBudgeter,
    clock: Arc<dyn Clock>,
    hot_target: Mutex<Option<RefetchTarget>>,
    mergeable_inflight: Mutex<HashSet<String>>,
    notifications_next_poll_at: Mutex<Option<tokio::time::Instant>>,
}

impl RealActions {
    pub fn new(
        db: Arc<Db>,
        github: GithubClient,
        account_id: String,
        locator: AccountLocator,
        budgeter: RateLimitBudgeter,
    ) -> Self {
        Self {
            inner: Arc::new(RealActionsInner {
                db,
                github,
                account_id,
                locator,
                budgeter,
                clock: Arc::new(TokioClock),
                hot_target: Mutex::new(None),
                mergeable_inflight: Mutex::new(HashSet::new()),
                notifications_next_poll_at: Mutex::new(None),
            }),
        }
    }

    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(RealActionsInner {
                db: Arc::clone(&self.inner.db),
                github: self.inner.github.clone(),
                account_id: self.inner.account_id.clone(),
                locator: self.inner.locator.clone(),
                budgeter: self.inner.budgeter.clone(),
                clock,
                hot_target: Mutex::new(None),
                mergeable_inflight: Mutex::new(HashSet::new()),
                notifications_next_poll_at: Mutex::new(None),
            }),
        }
    }

    pub async fn set_hot_target(&self, target: Option<RefetchTarget>) {
        *self.inner.hot_target.lock().await = target;
    }

    pub async fn refresh_inbox(
        &self,
        ids: &[String],
        priority: Priority,
    ) -> Result<Vec<RefetchTarget>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut targets = Vec::new();
        for chunk in ids.chunks(20) {
            let (payload, rate_limit) = self
                .inner
                .github
                .graphql::<InboxRefreshData>(
                    &self.inner.locator,
                    INBOX_REFRESH_QUERY,
                    serde_json::json!({ "ids": chunk }),
                )
                .await?;
            if let Some(rate_limit) = rate_limit {
                self.inner
                    .budgeter
                    .record(&self.inner.account_id, ApiResource::Graphql, rate_limit)
                    .await?;
            }
            let chunk_targets = reconcile_inbox_refresh(
                Arc::clone(&self.inner.db),
                &self.inner.account_id,
                payload,
            )
            .await?;
            targets.extend(chunk_targets);
        }
        if priority == Priority::Foreground {
            return Ok(targets);
        }
        Ok(targets)
    }

    async fn refresh_pr_detail_target(&self, target: &RefetchTarget) -> Result<bool> {
        let variables = serde_json::json!({
            "owner": target.owner,
            "repo": target.repo,
            "number": target.number,
            "timelineFirst": 50,
            "timelineAfter": serde_json::Value::Null,
            "threadFirst": 100,
            "reviewFirst": 100
        });
        let (payload, rate_limit) = self
            .inner
            .github
            .graphql::<PrDetailData>(&self.inner.locator, PR_DETAIL_QUERY, variables)
            .await?;
        if let Some(rate_limit) = rate_limit {
            self.inner
                .budgeter
                .record(&self.inner.account_id, ApiResource::Graphql, rate_limit)
                .await?;
        }
        let mergeable_is_null = payload
            .repository
            .as_ref()
            .and_then(|repo| repo.pull_request.as_ref())
            .map(|pr| pr.mergeable.is_none())
            .unwrap_or(false);

        if let Some(repo) = payload.repository.as_ref() {
            if let Some(pr) = repo.pull_request.as_ref() {
                let diff_response = self
                    .inner
                    .github
                    .fetch_pull_diff(PullDiffRequest {
                        account_id: &self.inner.account_id,
                        locator: &self.inner.locator,
                        owner: &target.owner,
                        repo: &target.repo,
                        number: target.number,
                        pr_id: &pr.id,
                        head_sha: &pr.head_ref_oid,
                    })
                    .await?;
                if let ConditionalResponse::Modified { metadata, .. } = diff_response {
                    if let Some(rate_limit) = metadata.rate_limit {
                        self.inner
                            .budgeter
                            .record(&self.inner.account_id, ApiResource::Core, rate_limit)
                            .await?;
                    }
                }
            }
        }

        let _ = reconcile_pr_detail(Arc::clone(&self.inner.db), &self.inner.account_id, payload)
            .await?;
        Ok(mergeable_is_null)
    }

    async fn poll_notifications(&self) -> Result<Vec<RefetchTarget>> {
        let now = tokio::time::Instant::now();
        if let Some(next_allowed) = *self.inner.notifications_next_poll_at.lock().await {
            if next_allowed > now {
                return Ok(Vec::new());
            }
        }

        let response = self
            .inner
            .github
            .poll_notifications(&self.inner.account_id, &self.inner.locator, false)
            .await?;
        match response {
            ConditionalResponse::NotModified(metadata) => {
                if let Some(interval) = metadata.poll_interval_seconds {
                    let mut next = self.inner.notifications_next_poll_at.lock().await;
                    *next = Some(tokio::time::Instant::now() + Duration::from_secs(interval));
                }
                Ok(Vec::new())
            }
            ConditionalResponse::Modified { payload, metadata } => {
                if let Some(rate_limit) = metadata.rate_limit {
                    self.inner
                        .budgeter
                        .record(&self.inner.account_id, ApiResource::Core, rate_limit)
                        .await?;
                }
                if let Some(interval) = metadata.poll_interval_seconds {
                    let mut next = self.inner.notifications_next_poll_at.lock().await;
                    *next = Some(tokio::time::Instant::now() + Duration::from_secs(interval));
                }
                let mapped = payload
                    .into_iter()
                    .map(map_notification_payload)
                    .collect::<Vec<_>>();
                reconcile_notifications(Arc::clone(&self.inner.db), &self.inner.account_id, mapped)
                    .await
            }
        }
    }

    fn schedule_mergeable_repoll(&self, target: RefetchTarget) {
        let key = format!("{}/{}/{}", target.owner, target.repo, target.number);
        let this = self.clone();
        tokio::spawn(async move {
            {
                let mut inflight = this.inner.mergeable_inflight.lock().await;
                if inflight.contains(&key) {
                    return;
                }
                inflight.insert(key.clone());
            }
            let _ = run_mergeable_backoff(this.inner.clock.as_ref(), || {
                let this = this.clone();
                let target = target.clone();
                async move { this.refresh_pr_detail_target(&target).await }
            })
            .await;
            let mut inflight = this.inner.mergeable_inflight.lock().await;
            inflight.remove(&key);
        });
    }
}

impl TierActions for RealActions {
    fn run_tier<'a>(
        &'a self,
        tier: Tier,
        _priority: Priority,
    ) -> ActionFuture<'a, Vec<RefetchTarget>> {
        Box::pin(async move {
            match tier {
                Tier::Hot => {
                    let target = self.inner.hot_target.lock().await.clone();
                    if let Some(target) = target {
                        let mergeable_null = self.refresh_pr_detail_target(&target).await?;
                        if mergeable_null {
                            self.schedule_mergeable_repoll(target);
                        }
                    }
                    Ok(Vec::new())
                }
                Tier::Warm => self.poll_notifications().await,
                Tier::Cool | Tier::Cold => Ok(Vec::new()),
            }
        })
    }

    fn run_refetch<'a>(
        &'a self,
        targets: Vec<RefetchTarget>,
        _priority: Priority,
    ) -> ActionFuture<'a, ()> {
        Box::pin(async move {
            for target in targets {
                let mergeable_null = self.refresh_pr_detail_target(&target).await?;
                if mergeable_null {
                    self.schedule_mergeable_repoll(target);
                }
            }
            Ok(())
        })
    }
}

pub async fn run_mergeable_backoff<F, Fut>(clock: &dyn Clock, mut poll: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool>> + Send,
{
    let schedule = [
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(15),
        Duration::from_secs(45),
        Duration::from_secs(120),
    ];
    for duration in schedule {
        clock.sleep(duration).await;
        let still_unknown = poll().await?;
        if !still_unknown {
            return Ok(());
        }
    }

    loop {
        let duration = Duration::from_secs(300);
        clock.sleep(duration).await;
        let still_unknown = poll().await?;
        if !still_unknown {
            return Ok(());
        }
    }
}

fn cadence_for(
    tier: Tier,
    focus_state: FocusState,
    unfocused_since: Option<tokio::time::Instant>,
) -> Option<Duration> {
    let unfocused_for =
        unfocused_since.map(|since| tokio::time::Instant::now().duration_since(since));
    tier_cadence(tier, focus_state, unfocused_for)
}

pub fn tier_cadence(
    tier: Tier,
    focus_state: FocusState,
    unfocused_for: Option<Duration>,
) -> Option<Duration> {
    if focus_state == FocusState::Unfocused
        && unfocused_for
            .map(|duration| duration >= UNFOCUS_PAUSE_AFTER)
            .unwrap_or(false)
    {
        return None;
    }
    match (tier, focus_state) {
        (Tier::Hot, FocusState::Focused) => Some(Duration::from_secs(5)),
        (Tier::Hot, FocusState::Unfocused) => None,
        (Tier::Warm, FocusState::Focused) => Some(Duration::from_secs(30)),
        (Tier::Warm, FocusState::Unfocused) => Some(Duration::from_secs(120)),
        (Tier::Cool, FocusState::Focused) => Some(Duration::from_secs(180)),
        (Tier::Cool, FocusState::Unfocused) => Some(Duration::from_secs(900)),
        (Tier::Cold, FocusState::Focused) => Some(Duration::from_secs(1800)),
        (Tier::Cold, FocusState::Unfocused) => None,
    }
}

fn map_notification_payload(notification: GithubNotification) -> NotificationPayload {
    NotificationPayload {
        id: notification.id,
        unread: notification.unread,
        reason: notification.reason,
        updated_at: notification.updated_at,
        last_read_at: notification.last_read_at,
        subject_title: notification.subject.title,
        subject_type: notification.subject.subject_type,
        subject_url: notification.subject.url,
        repository_id: notification.repository.id.to_string(),
        repository_owner: notification.repository.owner.login,
        repository_name: notification.repository.name,
        repository_html_url: notification.repository.html_url,
        repository_description: notification.repository.description,
        repository_private: notification.repository.private,
        repository_archived: notification.repository.archived.unwrap_or(false),
        url: notification.url,
    }
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}
