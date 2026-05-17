use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::Db;
use crate::mutations::{ErrorKind, MutationEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    ReviewRequested,
    ChangesRequested,
    Approved,
    Mention,
    CiFail,
    CiRecover,
    MergeConflict,
    MutationFailure,
}

impl NotificationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReviewRequested => "review_requested",
            Self::ChangesRequested => "changes_requested",
            Self::Approved => "approved",
            Self::Mention => "mention",
            Self::CiFail => "ci_fail",
            Self::CiRecover => "ci_recover",
            Self::MergeConflict => "merge_conflict",
            Self::MutationFailure => "mutation_failure",
        }
    }

    pub fn all() -> [NotificationKind; 8] {
        [
            Self::ReviewRequested,
            Self::ChangesRequested,
            Self::Approved,
            Self::Mention,
            Self::CiFail,
            Self::CiRecover,
            Self::MergeConflict,
            Self::MutationFailure,
        ]
    }
}

impl std::str::FromStr for NotificationKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "review_requested" => Ok(Self::ReviewRequested),
            "changes_requested" => Ok(Self::ChangesRequested),
            "approved" => Ok(Self::Approved),
            "mention" => Ok(Self::Mention),
            "ci_fail" => Ok(Self::CiFail),
            "ci_recover" => Ok(Self::CiRecover),
            "merge_conflict" => Ok(Self::MergeConflict),
            "mutation_failure" => Ok(Self::MutationFailure),
            other => Err(anyhow::anyhow!("unknown notification kind `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCandidate {
    pub account_id: String,
    pub repo_id: Option<String>,
    pub repo_full_name: Option<String>,
    pub pr_id: Option<String>,
    pub kind: NotificationKind,
    pub actor_id: String,
    pub server_event_id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub viewer_login: String,
    pub viewer_user_ids: HashSet<String>,
    pub pr_repo_ids: HashMap<String, String>,
    pub repo_full_names: HashMap<String, String>,
    pub authored_pr_ids: HashSet<String>,
    pub reviewing_pr_ids: HashSet<String>,
    pub viewer_review_requests: HashMap<String, ReviewRequestState>,
    pub reviews: HashMap<String, ReviewState>,
    pub comments: HashMap<String, CommentState>,
    pub check_states: HashMap<String, CheckState>,
    pub merge_states: HashMap<String, MergeState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRequestState {
    pub key: String,
    pub pr_id: String,
    pub reviewer_user_id: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewState {
    pub id: String,
    pub pr_id: String,
    pub author_id: String,
    pub state: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentState {
    pub id: String,
    pub pr_id: String,
    pub author_id: String,
    pub body: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckState {
    pub pr_id: String,
    pub is_red: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeState {
    pub pr_id: String,
    pub mergeable_state: Option<String>,
    pub updated_at: i64,
}

impl AccountSnapshot {
    pub async fn load(db: &Db, account_id: &str) -> Result<Option<Self>> {
        let viewer_login =
            sqlx::query_scalar::<_, String>("SELECT login FROM accounts WHERE id = ?1 LIMIT 1")
                .bind(account_id)
                .fetch_optional(db.pool())
                .await?;
        let Some(viewer_login) = viewer_login else {
            return Ok(None);
        };

        let mut viewer_user_ids = HashSet::new();
        let user_rows =
            sqlx::query("SELECT id FROM users WHERE account_id = ?1 AND lower(login) = lower(?2)")
                .bind(account_id)
                .bind(&viewer_login)
                .fetch_all(db.pool())
                .await?;
        for row in user_rows {
            viewer_user_ids.insert(row.try_get::<String, _>("id")?);
        }

        let mut pr_repo_ids = HashMap::new();
        let mut authored_pr_ids = HashSet::new();
        let pr_rows = sqlx::query(
            "SELECT id, repo_id, author_id, mergeable_state, updated_at
             FROM pull_requests
             WHERE account_id = ?1",
        )
        .bind(account_id)
        .fetch_all(db.pool())
        .await?;
        let mut merge_states = HashMap::new();
        for row in pr_rows {
            let pr_id = row.try_get::<String, _>("id")?;
            let repo_id = row.try_get::<String, _>("repo_id")?;
            let author_id = row.try_get::<Option<String>, _>("author_id")?;
            let mergeable_state = row.try_get::<Option<String>, _>("mergeable_state")?;
            let updated_at = row.try_get::<i64, _>("updated_at")?;
            if author_id
                .as_ref()
                .is_some_and(|id| viewer_user_ids.contains(id))
            {
                authored_pr_ids.insert(pr_id.clone());
            }
            pr_repo_ids.insert(pr_id.clone(), repo_id);
            merge_states.insert(
                pr_id.clone(),
                MergeState {
                    pr_id,
                    mergeable_state,
                    updated_at,
                },
            );
        }

        let mut repo_full_names = HashMap::new();
        let repo_rows = sqlx::query("SELECT id, owner, name FROM repos WHERE account_id = ?1")
            .bind(account_id)
            .fetch_all(db.pool())
            .await?;
        for row in repo_rows {
            let repo_id = row.try_get::<String, _>("id")?;
            let owner = row.try_get::<String, _>("owner")?;
            let name = row.try_get::<String, _>("name")?;
            repo_full_names.insert(repo_id, format!("{owner}/{name}"));
        }

        let mut viewer_review_requests = HashMap::new();
        let request_rows = sqlx::query(
            "SELECT pr_id, user_id, requested_at
             FROM pr_reviewers
             WHERE account_id = ?1
               AND reviewer_type = 'user'",
        )
        .bind(account_id)
        .fetch_all(db.pool())
        .await?;
        for row in request_rows {
            let user_id = row.try_get::<String, _>("user_id")?;
            if !viewer_user_ids.contains(&user_id) {
                continue;
            }
            let pr_id = row.try_get::<String, _>("pr_id")?;
            let requested_at = row.try_get::<i64, _>("requested_at")?;
            let key = format!("{pr_id}:{user_id}");
            viewer_review_requests.insert(
                key.clone(),
                ReviewRequestState {
                    key,
                    pr_id,
                    reviewer_user_id: user_id,
                    requested_at,
                },
            );
        }

        let mut reviews = HashMap::new();
        let mut reviewing_pr_ids = HashSet::new();
        let review_rows = sqlx::query(
            "SELECT id, pr_id, author_id, state, updated_at
             FROM reviews
             WHERE account_id = ?1",
        )
        .bind(account_id)
        .fetch_all(db.pool())
        .await?;
        for row in review_rows {
            let id = row.try_get::<String, _>("id")?;
            let pr_id = row.try_get::<String, _>("pr_id")?;
            let author_id = row.try_get::<String, _>("author_id")?;
            let state = row.try_get::<String, _>("state")?;
            let updated_at = row.try_get::<i64, _>("updated_at")?;
            if viewer_user_ids.contains(&author_id) {
                reviewing_pr_ids.insert(pr_id.clone());
            }
            reviews.insert(
                id.clone(),
                ReviewState {
                    id,
                    pr_id,
                    author_id,
                    state,
                    updated_at,
                },
            );
        }
        reviewing_pr_ids.extend(
            viewer_review_requests
                .values()
                .map(|value| value.pr_id.clone()),
        );

        let mut comments = HashMap::new();
        let comment_rows = sqlx::query(
            "SELECT id, pr_id, author_id, body, updated_at
             FROM comments
             WHERE account_id = ?1",
        )
        .bind(account_id)
        .fetch_all(db.pool())
        .await?;
        for row in comment_rows {
            let id = row.try_get::<String, _>("id")?;
            comments.insert(
                id.clone(),
                CommentState {
                    id,
                    pr_id: row.try_get::<String, _>("pr_id")?,
                    author_id: row.try_get::<String, _>("author_id")?,
                    body: row.try_get::<String, _>("body")?,
                    updated_at: row.try_get::<i64, _>("updated_at")?,
                },
            );
        }

        let check_rows = sqlx::query(
            "SELECT
               pr_id,
               COALESCE(MAX(updated_at), 0) AS updated_at,
               SUM(CASE
                     WHEN lower(COALESCE(conclusion, '')) IN (
                       'failure', 'timed_out', 'cancelled', 'action_required', 'startup_failure'
                     )
                     THEN 1 ELSE 0
                   END) AS failing_count
             FROM check_suites
             WHERE account_id = ?1
             GROUP BY pr_id",
        )
        .bind(account_id)
        .fetch_all(db.pool())
        .await?;
        let mut check_states = HashMap::new();
        for row in check_rows {
            let pr_id = row.try_get::<String, _>("pr_id")?;
            let failing_count = row.try_get::<i64, _>("failing_count")?;
            check_states.insert(
                pr_id.clone(),
                CheckState {
                    pr_id,
                    is_red: failing_count > 0,
                    updated_at: row.try_get::<i64, _>("updated_at")?,
                },
            );
        }

        Ok(Some(Self {
            account_id: account_id.to_string(),
            viewer_login,
            viewer_user_ids,
            pr_repo_ids,
            repo_full_names,
            authored_pr_ids,
            reviewing_pr_ids,
            viewer_review_requests,
            reviews,
            comments,
            check_states,
            merge_states,
        }))
    }
}

pub trait Trigger: Send + Sync {
    fn kind(&self) -> NotificationKind;
    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate>;
}

pub fn sync_triggers() -> Vec<Box<dyn Trigger>> {
    vec![
        Box::new(ReviewRequestedTrigger),
        Box::new(ChangesRequestedTrigger),
        Box::new(ApprovedTrigger),
        Box::new(MentionTrigger),
        Box::new(CiFailTrigger),
        Box::new(CiRecoverTrigger),
        Box::new(MergeConflictTrigger),
    ]
}

pub fn mutation_failure_candidate(
    event: &MutationEvent,
    account_id: &str,
    repo_id: Option<String>,
    repo_full_name: Option<String>,
    pr_id: Option<String>,
) -> Option<NotificationCandidate> {
    let MutationEvent::Failed {
        mutation_id,
        error_kind,
        ..
    } = event
    else {
        return None;
    };

    if matches!(error_kind, ErrorKind::Network) {
        return None;
    }

    Some(NotificationCandidate {
        account_id: account_id.to_string(),
        repo_id,
        repo_full_name,
        pr_id,
        kind: NotificationKind::MutationFailure,
        actor_id: account_id.to_string(),
        server_event_id: mutation_id.clone(),
        title: "Mutation failed".to_string(),
        body: format!("A queued mutation failed and needs attention ({mutation_id})."),
    })
}

struct ReviewRequestedTrigger;

impl ReviewRequestedTrigger {
    fn predicate(old: &AccountSnapshot, next: &ReviewRequestState) -> bool {
        old.viewer_review_requests
            .get(&next.key)
            .map(|current| current.requested_at < next.requested_at)
            .unwrap_or(true)
    }

    fn fire(next: &ReviewRequestState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(&next.pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(next.pr_id.clone()),
            kind: NotificationKind::ReviewRequested,
            actor_id: "system".to_string(),
            server_event_id: format!(
                "{}:{}:{}",
                next.pr_id, next.reviewer_user_id, next.requested_at
            ),
            title: "Review requested".to_string(),
            body: format!("A pull request requested your review ({})", next.pr_id),
        }
    }
}

impl Trigger for ReviewRequestedTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::ReviewRequested
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        new.viewer_review_requests
            .values()
            .filter(|next| Self::predicate(old, next))
            .map(|next| Self::fire(next, new))
            .collect()
    }
}

struct ChangesRequestedTrigger;

impl ChangesRequestedTrigger {
    fn predicate(
        old: &AccountSnapshot,
        next: &ReviewState,
        viewer_user_ids: &HashSet<String>,
    ) -> bool {
        if !next.state.eq_ignore_ascii_case("CHANGES_REQUESTED")
            || viewer_user_ids.contains(&next.author_id)
        {
            return false;
        }
        old.reviews
            .get(&next.id)
            .map(|current| {
                !current.state.eq_ignore_ascii_case("CHANGES_REQUESTED")
                    || current.updated_at < next.updated_at
            })
            .unwrap_or(true)
    }

    fn fire(next: &ReviewState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(&next.pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(next.pr_id.clone()),
            kind: NotificationKind::ChangesRequested,
            actor_id: next.author_id.clone(),
            server_event_id: next.id.clone(),
            title: "Changes requested".to_string(),
            body: format!("A reviewer requested changes on {}.", next.pr_id),
        }
    }
}

impl Trigger for ChangesRequestedTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::ChangesRequested
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        new.reviews
            .values()
            .filter(|next| Self::predicate(old, next, &new.viewer_user_ids))
            .map(|next| Self::fire(next, new))
            .collect()
    }
}

struct ApprovedTrigger;

impl ApprovedTrigger {
    fn predicate(
        old: &AccountSnapshot,
        next: &ReviewState,
        viewer_user_ids: &HashSet<String>,
    ) -> bool {
        if !next.state.eq_ignore_ascii_case("APPROVED") || viewer_user_ids.contains(&next.author_id)
        {
            return false;
        }
        old.reviews
            .get(&next.id)
            .map(|current| {
                !current.state.eq_ignore_ascii_case("APPROVED")
                    || current.updated_at < next.updated_at
            })
            .unwrap_or(true)
    }

    fn fire(next: &ReviewState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(&next.pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(next.pr_id.clone()),
            kind: NotificationKind::Approved,
            actor_id: next.author_id.clone(),
            server_event_id: next.id.clone(),
            title: "Pull request approved".to_string(),
            body: format!("A reviewer approved {}.", next.pr_id),
        }
    }
}

impl Trigger for ApprovedTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::Approved
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        new.reviews
            .values()
            .filter(|next| Self::predicate(old, next, &new.viewer_user_ids))
            .map(|next| Self::fire(next, new))
            .collect()
    }
}

struct MentionTrigger;

impl MentionTrigger {
    fn predicate(old: &AccountSnapshot, next: &CommentState, mention_token: &str) -> bool {
        if !next.body.to_ascii_lowercase().contains(mention_token) {
            return false;
        }
        old.comments
            .get(&next.id)
            .map(|current| current.updated_at < next.updated_at || current.body != next.body)
            .unwrap_or(true)
    }

    fn fire(next: &CommentState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(&next.pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(next.pr_id.clone()),
            kind: NotificationKind::Mention,
            actor_id: next.author_id.clone(),
            server_event_id: next.id.clone(),
            title: "You were mentioned".to_string(),
            body: format!("A comment mentioned @{}.", snapshot.viewer_login),
        }
    }
}

impl Trigger for MentionTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::Mention
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        let mention_token = format!("@{}", new.viewer_login.to_ascii_lowercase());
        new.comments
            .values()
            .filter(|next| {
                !new.viewer_user_ids.contains(&next.author_id)
                    && Self::predicate(old, next, &mention_token)
            })
            .map(|next| Self::fire(next, new))
            .collect()
    }
}

struct CiFailTrigger;

impl CiFailTrigger {
    fn predicate(old: &AccountSnapshot, pr_id: &str, next: &CheckState) -> bool {
        old.check_states
            .get(pr_id)
            .map(|prev| !prev.is_red && next.is_red)
            .unwrap_or(false)
    }

    fn fire(pr_id: &str, next: &CheckState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(pr_id.to_string()),
            kind: NotificationKind::CiFail,
            actor_id: "system".to_string(),
            server_event_id: format!("{pr_id}:{}:ci_fail", next.updated_at),
            title: "CI failed".to_string(),
            body: format!("Checks turned red on {pr_id}."),
        }
    }
}

impl Trigger for CiFailTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::CiFail
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        let involved = new
            .authored_pr_ids
            .union(&new.reviewing_pr_ids)
            .cloned()
            .collect::<HashSet<_>>();
        new.check_states
            .iter()
            .filter(|(pr_id, next)| involved.contains(*pr_id) && Self::predicate(old, pr_id, next))
            .map(|(pr_id, next)| Self::fire(pr_id, next, new))
            .collect()
    }
}

struct CiRecoverTrigger;

impl CiRecoverTrigger {
    fn predicate(old: &AccountSnapshot, pr_id: &str, next: &CheckState) -> bool {
        old.check_states
            .get(pr_id)
            .map(|prev| prev.is_red && !next.is_red)
            .unwrap_or(false)
    }

    fn fire(pr_id: &str, next: &CheckState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(pr_id.to_string()),
            kind: NotificationKind::CiRecover,
            actor_id: "system".to_string(),
            server_event_id: format!("{pr_id}:{}:ci_recover", next.updated_at),
            title: "CI recovered".to_string(),
            body: format!("Checks are green again on {pr_id}."),
        }
    }
}

impl Trigger for CiRecoverTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::CiRecover
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        let involved = new
            .authored_pr_ids
            .union(&new.reviewing_pr_ids)
            .cloned()
            .collect::<HashSet<_>>();
        new.check_states
            .iter()
            .filter(|(pr_id, next)| involved.contains(*pr_id) && Self::predicate(old, pr_id, next))
            .map(|(pr_id, next)| Self::fire(pr_id, next, new))
            .collect()
    }
}

struct MergeConflictTrigger;

impl MergeConflictTrigger {
    fn predicate(old: &AccountSnapshot, pr_id: &str, next: &MergeState) -> bool {
        let next_dirty = next
            .mergeable_state
            .as_ref()
            .is_some_and(|state| state.eq_ignore_ascii_case("dirty"));
        if !next_dirty {
            return false;
        }
        old.merge_states
            .get(pr_id)
            .map(|prev| {
                !prev
                    .mergeable_state
                    .as_ref()
                    .is_some_and(|state| state.eq_ignore_ascii_case("dirty"))
            })
            .unwrap_or(false)
    }

    fn fire(pr_id: &str, next: &MergeState, snapshot: &AccountSnapshot) -> NotificationCandidate {
        let repo_id = snapshot.pr_repo_ids.get(pr_id).cloned();
        let repo_full_name = repo_id
            .as_ref()
            .and_then(|id| snapshot.repo_full_names.get(id))
            .cloned();
        NotificationCandidate {
            account_id: snapshot.account_id.clone(),
            repo_id,
            repo_full_name,
            pr_id: Some(pr_id.to_string()),
            kind: NotificationKind::MergeConflict,
            actor_id: "system".to_string(),
            server_event_id: format!("{pr_id}:{}:merge_conflict", next.updated_at),
            title: "Merge conflict detected".to_string(),
            body: format!("{pr_id} is now in a merge-conflict state."),
        }
    }
}

impl Trigger for MergeConflictTrigger {
    fn kind(&self) -> NotificationKind {
        NotificationKind::MergeConflict
    }

    fn evaluate(&self, old: &AccountSnapshot, new: &AccountSnapshot) -> Vec<NotificationCandidate> {
        new.merge_states
            .iter()
            .filter(|(pr_id, state)| Self::predicate(old, pr_id, state))
            .map(|(pr_id, state)| Self::fire(pr_id, state, new))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationFailureContext {
    pub account_id: String,
    pub repo_id: Option<String>,
    pub repo_full_name: Option<String>,
    pub pr_id: Option<String>,
}

pub async fn mutation_failure_context(
    db: &Db,
    mutation_id: &str,
) -> Result<Option<MutationFailureContext>> {
    let row = sqlx::query(
        "SELECT
           pm.account_id AS account_id,
           CASE WHEN pm.target_type = 'pull_request' THEN pm.target_id ELSE NULL END AS pr_id,
           pr.repo_id AS repo_id
         FROM pending_mutations pm
         LEFT JOIN pull_requests pr
           ON pr.account_id = pm.account_id
          AND pr.id = pm.target_id
          AND pm.target_type = 'pull_request'
         WHERE pm.id = ?1
         LIMIT 1",
    )
    .bind(mutation_id)
    .fetch_optional(db.pool())
    .await
    .with_context(|| format!("loading mutation failure context for `{mutation_id}`"))?;
    let Some(row) = row else {
        return Ok(None);
    };

    let account_id = row.try_get::<String, _>("account_id")?;
    let repo_id = row.try_get::<Option<String>, _>("repo_id")?;
    let repo_full_name = if let Some(repo_id) = repo_id.as_ref() {
        sqlx::query_scalar::<_, String>(
            "SELECT owner || '/' || name FROM repos WHERE id = ?1 LIMIT 1",
        )
        .bind(repo_id)
        .fetch_optional(db.pool())
        .await?
    } else {
        None
    };

    Ok(Some(MutationFailureContext {
        account_id,
        repo_id,
        repo_full_name,
        pr_id: row.try_get::<Option<String>, _>("pr_id")?,
    }))
}
