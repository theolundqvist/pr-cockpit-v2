use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::DateTime;
use serde::Deserialize;

use crate::db::{
    CheckRunRecord, CheckSuiteRecord, CommentRecord, Db, NotificationRecord, PrAssigneeRecord,
    PrLabelRecord, PrPushRecord, PrReviewerRecord, PullRequestRecord, RepoRecord, ReviewRecord,
    ReviewThreadRecord, UserRecord,
};

#[derive(Debug, Deserialize)]
pub struct PrDetailData {
    pub repository: Option<PrDetailRepository>,
}

#[derive(Debug, Deserialize)]
pub struct PrDetailRepository {
    pub id: String,
    pub owner: PrOwner,
    pub name: String,
    #[serde(rename = "pullRequest")]
    pub pull_request: Option<PrDetailNode>,
}

#[derive(Debug, Deserialize)]
pub struct PrOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct PrDetailNode {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    pub mergeable: Option<String>,
    #[serde(rename = "mergeStateStatus")]
    pub merge_state_status: Option<String>,
    #[serde(rename = "viewerCanMerge")]
    pub viewer_can_merge: Option<bool>,
    #[serde(rename = "viewerCanEnableAutoMerge")]
    pub viewer_can_enable_auto_merge: Option<bool>,
    #[serde(rename = "viewerCanDisableAutoMerge")]
    pub viewer_can_disable_auto_merge: Option<bool>,
    #[serde(rename = "viewerCanUpdateBranch")]
    pub viewer_can_update_branch: Option<bool>,
    #[serde(rename = "viewerCanDeleteHeadRef")]
    pub viewer_can_delete_head_ref: Option<bool>,
    #[serde(rename = "autoMergeRequest")]
    pub auto_merge_request: Option<AutoMergeRequestNode>,
    #[serde(rename = "mergeQueueEntry")]
    pub merge_queue_entry: Option<MergeQueueEntryNode>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<String>,
    #[serde(rename = "mergedAt")]
    pub merged_at: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    #[serde(rename = "changedFiles")]
    pub changed_files: i64,
    pub comments: CountNode,
    pub commits: PrCommitsConnection,
    pub author: Option<GraphqlUser>,
    #[serde(rename = "baseRefName")]
    pub base_ref_name: String,
    #[serde(rename = "baseRefOid")]
    pub base_ref_oid: String,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    #[serde(rename = "headRef")]
    pub head_ref: Option<HeadRefNode>,
    #[serde(rename = "headRefOid")]
    pub head_ref_oid: String,
    pub repository: Option<PullRequestRepositoryNode>,
    #[serde(rename = "headRepository")]
    pub head_repository: Option<HeadRepositoryNode>,
    pub labels: LabelsConnection,
    pub assignees: UsersConnection,
    #[serde(rename = "reviewRequests")]
    pub review_requests: ReviewRequestsConnection,
    #[serde(rename = "reviewThreads")]
    pub review_threads: ReviewThreadsConnection,
    pub reviews: ReviewsConnection,
    #[serde(rename = "timelineItems")]
    pub timeline_items: TimelineConnection,
}

#[derive(Debug, Deserialize)]
pub struct AutoMergeRequestNode {
    #[serde(rename = "mergeMethod")]
    pub merge_method: Option<String>,
    #[serde(rename = "commitHeadline")]
    pub commit_headline: Option<String>,
    #[serde(rename = "commitBody")]
    pub commit_body: Option<String>,
    #[serde(rename = "enabledAt")]
    pub enabled_at: Option<String>,
    #[serde(rename = "enabledBy")]
    pub enabled_by: Option<GraphqlUser>,
}

#[derive(Debug, Deserialize)]
pub struct MergeQueueEntryNode {
    pub id: Option<String>,
    pub position: Option<i64>,
    pub state: Option<String>,
    #[serde(rename = "estimatedTimeToMerge")]
    pub estimated_time_to_merge: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct HeadRefNode {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PullRequestRepositoryNode {
    #[serde(rename = "mergeCommitAllowed")]
    pub merge_commit_allowed: Option<bool>,
    #[serde(rename = "squashMergeAllowed")]
    pub squash_merge_allowed: Option<bool>,
    #[serde(rename = "rebaseMergeAllowed")]
    pub rebase_merge_allowed: Option<bool>,
    #[serde(rename = "deleteBranchOnMerge")]
    pub delete_branch_on_merge: Option<bool>,
    #[serde(rename = "mergeQueue")]
    pub merge_queue: Option<MergeQueueNode>,
    #[serde(rename = "defaultBranchRef")]
    pub default_branch_ref: Option<DefaultBranchRefNode>,
}

#[derive(Debug, Deserialize)]
pub struct MergeQueueNode {
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DefaultBranchRefNode {
    #[serde(rename = "branchProtectionRule")]
    pub branch_protection_rule: Option<BranchProtectionRuleNode>,
}

#[derive(Debug, Deserialize)]
pub struct BranchProtectionRuleNode {
    #[serde(rename = "requiresApprovingReviews")]
    pub requires_approving_reviews: Option<bool>,
    #[serde(rename = "requiredApprovingReviewCount")]
    pub required_approving_review_count: Option<i64>,
    #[serde(rename = "requiresStatusChecks")]
    pub requires_status_checks: Option<bool>,
    #[serde(rename = "requiredStatusCheckContexts")]
    pub required_status_check_contexts: Option<Vec<String>>,
    #[serde(rename = "requiresStrictStatusChecks")]
    pub requires_strict_status_checks: Option<bool>,
    #[serde(rename = "restrictsPushes")]
    pub restricts_pushes: Option<bool>,
    #[serde(rename = "restrictsReviewDismissals")]
    pub restricts_review_dismissals: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CountNode {
    #[serde(rename = "totalCount")]
    pub total_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct PrCommitsConnection {
    #[serde(rename = "totalCount")]
    pub total_count: i64,
    pub nodes: Option<Vec<Option<PrCommitNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct PrCommitNode {
    pub commit: CommitNode,
}

#[derive(Debug, Deserialize)]
pub struct CommitNode {
    pub oid: String,
    #[serde(rename = "checkSuites")]
    pub check_suites: Option<CheckSuitesConnection>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSuitesConnection {
    pub nodes: Option<Vec<Option<CheckSuiteNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSuiteNode {
    pub id: String,
    pub app: Option<CheckSuiteApp>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    #[serde(rename = "workflowRun")]
    pub workflow_run: Option<WorkflowRunNode>,
    #[serde(rename = "checkRuns")]
    pub check_runs: Option<CheckRunsConnection>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSuiteApp {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRunNode {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunsConnection {
    pub nodes: Option<Vec<Option<CheckRunNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunNode {
    pub id: String,
    pub name: String,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    #[serde(rename = "detailsUrl")]
    pub details_url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HeadRepositoryNode {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct LabelsConnection {
    pub nodes: Option<Vec<Option<LabelNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct LabelNode {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UsersConnection {
    pub nodes: Option<Vec<Option<GraphqlUser>>>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequestsConnection {
    pub nodes: Option<Vec<Option<ReviewRequestNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequestNode {
    #[serde(rename = "requestedReviewer")]
    pub requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum RequestedReviewer {
    User(GraphqlUser),
    Team(GraphqlTeam),
}

#[derive(Debug, Deserialize)]
pub struct GraphqlTeam {
    pub id: String,
    pub slug: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GraphqlUser {
    pub id: Option<String>,
    pub login: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewThreadsConnection {
    pub nodes: Option<Vec<Option<ReviewThreadNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewThreadNode {
    pub id: String,
    #[serde(rename = "isOutdated")]
    pub is_outdated: bool,
    #[serde(rename = "isResolved")]
    pub is_resolved: bool,
    #[serde(rename = "resolvedBy")]
    pub resolved_by: Option<GraphqlUser>,
    pub path: String,
    pub line: Option<i64>,
    pub side: Option<String>,
    #[serde(rename = "startLine")]
    pub start_line: Option<i64>,
    #[serde(rename = "startSide")]
    pub start_side: Option<String>,
    #[serde(rename = "originalCommit")]
    pub original_commit: Option<OriginalCommitNode>,
    #[serde(rename = "originalLine")]
    pub original_line: Option<i64>,
    #[serde(rename = "originalStartLine")]
    pub original_start_line: Option<i64>,
    pub comments: ThreadCommentsConnection,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OriginalCommitNode {
    pub oid: String,
}

#[derive(Debug, Deserialize)]
pub struct ThreadCommentsConnection {
    pub nodes: Option<Vec<Option<ThreadCommentNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct ThreadCommentNode {
    pub id: String,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub author: Option<GraphqlUser>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewsConnection {
    pub nodes: Option<Vec<Option<ReviewNode>>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReviewNode {
    pub id: String,
    pub state: String,
    pub body: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<String>,
    pub commit: Option<OriginalCommitNode>,
    pub author: Option<GraphqlUser>,
}

#[derive(Debug, Deserialize)]
pub struct TimelineConnection {
    pub nodes: Option<Vec<Option<TimelineItem>>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum TimelineItem {
    IssueComment(TimelineIssueComment),
    PullRequestReview(ReviewNode),
}

#[derive(Debug, Deserialize)]
pub struct TimelineIssueComment {
    pub id: String,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub author: Option<GraphqlUser>,
}

#[derive(Debug, Deserialize)]
pub struct InboxRefreshData {
    pub nodes: Vec<Option<InboxNode>>,
}

#[derive(Debug, Deserialize)]
pub struct InboxNode {
    #[serde(rename = "__typename")]
    pub typename: String,
    pub id: Option<String>,
    pub number: Option<i64>,
    pub title: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "isDraft")]
    pub is_draft: Option<bool>,
    pub mergeable: Option<String>,
    pub author: Option<GraphqlUser>,
    pub repository: Option<InboxRepository>,
    #[serde(rename = "headRefOid")]
    pub head_ref_oid: Option<String>,
    #[serde(rename = "baseRefOid")]
    pub base_ref_oid: Option<String>,
    pub labels: Option<LabelsConnection>,
    #[serde(rename = "reviewRequests")]
    pub review_requests: Option<ReviewRequestsConnection>,
    pub commits: Option<InboxCommitRollupConnection>,
    #[serde(rename = "unreadPlaceholder")]
    pub unread_placeholder: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct InboxRepository {
    pub id: String,
    pub owner: PrOwner,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct InboxCommitRollupConnection {
    pub nodes: Option<Vec<Option<InboxCommitNode>>>,
}

#[derive(Debug, Deserialize)]
pub struct InboxCommitNode {
    pub commit: InboxCommitPayload,
}

#[derive(Debug, Deserialize)]
pub struct InboxCommitPayload {
    #[serde(rename = "statusCheckRollup")]
    pub status_check_rollup: Option<InboxStatusRollup>,
}

#[derive(Debug, Deserialize)]
pub struct InboxStatusRollup {
    pub state: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefetchTarget {
    pub owner: String,
    pub repo: String,
    pub number: i64,
}

#[derive(Debug, Clone)]
pub struct NotificationPayload {
    pub id: String,
    pub unread: bool,
    pub reason: String,
    pub updated_at: String,
    pub last_read_at: Option<String>,
    pub subject_title: String,
    pub subject_type: String,
    pub subject_url: Option<String>,
    pub repository_id: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub repository_html_url: Option<String>,
    pub repository_description: Option<String>,
    pub repository_private: bool,
    pub repository_archived: bool,
    pub url: Option<String>,
}

pub async fn reconcile_pr_detail(
    db: Arc<Db>,
    account_id: &str,
    data: PrDetailData,
) -> Result<Option<RefetchTarget>> {
    let Some(repository) = data.repository else {
        return Ok(None);
    };
    let Some(pr) = repository.pull_request else {
        return Ok(None);
    };

    let now = now_epoch_seconds()?;
    let previous_push = db.latest_pr_push(&pr.id).await?;
    db.upsert_repo(&RepoRecord {
        id: repository.id.clone(),
        account_id: account_id.to_string(),
        owner: repository.owner.login.clone(),
        name: repository.name.clone(),
        default_branch: None,
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: now,
        updated_at: parse_timestamp(&pr.updated_at),
    })
    .await?;

    let commit_oids = pr
        .commits
        .nodes
        .as_ref()
        .map(|nodes| {
            nodes
                .iter()
                .flatten()
                .map(|node| node.commit.oid.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let pending_push_record = build_push_record(
        &pr.id,
        account_id,
        &pr.base_ref_oid,
        &pr.head_ref_oid,
        now,
        previous_push.as_ref(),
        &commit_oids,
    );

    let author_id = if let Some(author) = &pr.author {
        upsert_user_if_present(
            db.clone(),
            account_id,
            author,
            parse_timestamp(&pr.updated_at),
        )
        .await?
    } else {
        None
    };

    db.upsert_pull_request(&PullRequestRecord {
        id: pr.id.clone(),
        account_id: account_id.to_string(),
        repo_id: repository.id.clone(),
        number: pr.number,
        state: pr.state.to_lowercase(),
        draft: pr.is_draft,
        title: pr.title.clone(),
        body: pr.body.clone(),
        author_id,
        base_ref: pr.base_ref_name.clone(),
        base_sha: pr.base_ref_oid.clone(),
        head_ref: pr
            .head_ref
            .as_ref()
            .and_then(|head_ref| head_ref.name.clone())
            .unwrap_or_else(|| pr.head_ref_name.clone()),
        head_sha: pr.head_ref_oid.clone(),
        head_repo_id: pr.head_repository.as_ref().map(|repo| repo.id.clone()),
        mergeable_state: pr.mergeable.clone(),
        merge_state_status: pr.merge_state_status.clone(),
        merge_commit_allowed: pr
            .repository
            .as_ref()
            .and_then(|repo| repo.merge_commit_allowed),
        squash_merge_allowed: pr
            .repository
            .as_ref()
            .and_then(|repo| repo.squash_merge_allowed),
        rebase_merge_allowed: pr
            .repository
            .as_ref()
            .and_then(|repo| repo.rebase_merge_allowed),
        delete_branch_on_merge_default: pr
            .repository
            .as_ref()
            .and_then(|repo| repo.delete_branch_on_merge),
        viewer_can_merge: pr.viewer_can_merge,
        viewer_can_enable_auto_merge: pr.viewer_can_enable_auto_merge,
        viewer_can_disable_auto_merge: pr.viewer_can_disable_auto_merge,
        viewer_can_update_branch: pr.viewer_can_update_branch,
        viewer_can_delete_head_ref: pr.viewer_can_delete_head_ref,
        auto_merge_enabled: Some(pr.auto_merge_request.is_some()),
        auto_merge_method: pr
            .auto_merge_request
            .as_ref()
            .and_then(|request| request.merge_method.clone()),
        auto_merge_commit_headline: pr
            .auto_merge_request
            .as_ref()
            .and_then(|request| request.commit_headline.clone()),
        auto_merge_commit_body: pr
            .auto_merge_request
            .as_ref()
            .and_then(|request| request.commit_body.clone()),
        auto_merge_enabled_by_login: pr
            .auto_merge_request
            .as_ref()
            .and_then(|request| request.enabled_by.as_ref())
            .and_then(|user| user.login.clone()),
        auto_merge_enabled_at: pr
            .auto_merge_request
            .as_ref()
            .and_then(|request| request.enabled_at.as_deref().map(parse_timestamp)),
        merge_queue_entry_id: pr
            .merge_queue_entry
            .as_ref()
            .and_then(|entry| entry.id.clone()),
        merge_queue_entry_position: pr
            .merge_queue_entry
            .as_ref()
            .and_then(|entry| entry.position),
        merge_queue_entry_state: pr
            .merge_queue_entry
            .as_ref()
            .and_then(|entry| entry.state.clone()),
        merge_queue_entry_estimated_ms: pr
            .merge_queue_entry
            .as_ref()
            .and_then(|entry| entry.estimated_time_to_merge)
            .map(|seconds| seconds.saturating_mul(1000)),
        branch_protection_summary_json: pr
            .repository
            .as_ref()
            .and_then(|repo| repo.default_branch_ref.as_ref())
            .and_then(|branch| branch.branch_protection_rule.as_ref())
            .map(|rule| {
                serde_json::json!({
                    "requires_approving_reviews": rule.requires_approving_reviews.unwrap_or(false),
                    "required_approving_review_count": rule.required_approving_review_count.unwrap_or(0),
                    "requires_status_checks": rule.requires_status_checks.unwrap_or(false),
                    "required_status_check_contexts": rule.required_status_check_contexts.clone().unwrap_or_default(),
                    "requires_strict_status_checks": rule.requires_strict_status_checks.unwrap_or(false),
                    "restricts_pushes": rule.restricts_pushes.unwrap_or(false),
                    "restricts_review_dismissals": rule.restricts_review_dismissals.unwrap_or(false),
                })
                .to_string()
            }),
        repo_has_merge_queue: pr
            .repository
            .as_ref()
            .map(|repo| repo.merge_queue.as_ref().and_then(|queue| queue.id.clone()).is_some()),
        head_ref_state: Some(if pr.head_ref.is_some() {
            "ACTIVE".to_string()
        } else {
            "DELETED".to_string()
        }),
        additions: pr.additions,
        deletions: pr.deletions,
        changed_files: pr.changed_files,
        comments_count: pr.comments.total_count,
        reviews_count: pr
            .reviews
            .nodes
            .as_ref()
            .map_or(0_i64, |nodes| i64::try_from(nodes.len()).unwrap_or(0)),
        commits_count: pr.commits.total_count,
        is_read: true,
        html_url: None,
        created_at: parse_timestamp(&pr.created_at),
        updated_at: parse_timestamp(&pr.updated_at),
        closed_at: pr.closed_at.as_deref().map(parse_timestamp),
        merged_at: pr.merged_at.as_deref().map(parse_timestamp),
    })
    .await?;
    if let Some(push_record) = pending_push_record {
        db.upsert_pr_push(&push_record).await?;
    }

    let labels = pr
        .labels
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|label| PrLabelRecord {
            account_id: account_id.to_string(),
            pr_id: pr.id.clone(),
            label_name: label.name,
            label_color: label.color,
            description: label.description,
        })
        .collect::<Vec<_>>();
    db.replace_pr_labels(account_id, &pr.id, &labels).await?;

    let mut assignees = Vec::new();
    for user in pr.assignees.nodes.unwrap_or_default().into_iter().flatten() {
        if let Some(user_id) = upsert_user_if_present(
            db.clone(),
            account_id,
            &user,
            parse_timestamp(&pr.updated_at),
        )
        .await?
        {
            assignees.push(PrAssigneeRecord {
                account_id: account_id.to_string(),
                pr_id: pr.id.clone(),
                user_id,
                assigned_at: parse_timestamp(&pr.updated_at),
            });
        }
    }
    db.replace_pr_assignees(account_id, &pr.id, &assignees)
        .await?;

    let mut reviewers = Vec::new();
    for request in pr
        .review_requests
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
    {
        let Some(requested) = request.requested_reviewer else {
            continue;
        };
        match requested {
            RequestedReviewer::User(user) => {
                if let Some(user_id) = upsert_user_if_present(
                    db.clone(),
                    account_id,
                    &user,
                    parse_timestamp(&pr.updated_at),
                )
                .await?
                {
                    reviewers.push(PrReviewerRecord {
                        account_id: account_id.to_string(),
                        pr_id: pr.id.clone(),
                        user_id,
                        reviewer_type: "user".to_string(),
                        reviewer_state: "requested".to_string(),
                        requested_at: parse_timestamp(&pr.updated_at),
                    });
                }
            }
            RequestedReviewer::Team(team) => {
                reviewers.push(PrReviewerRecord {
                    account_id: account_id.to_string(),
                    pr_id: pr.id.clone(),
                    user_id: team.id,
                    reviewer_type: "team".to_string(),
                    reviewer_state: "requested".to_string(),
                    requested_at: parse_timestamp(&pr.updated_at),
                });
            }
        }
    }
    db.replace_pr_reviewers(account_id, &pr.id, &reviewers)
        .await?;

    let mut review_map = HashMap::<String, ReviewNode>::new();
    for review in pr.reviews.nodes.unwrap_or_default().into_iter().flatten() {
        review_map.insert(review.id.clone(), review);
    }
    for item in pr
        .timeline_items
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
    {
        match item {
            TimelineItem::IssueComment(comment) => {
                if let Some(author) = comment.author {
                    if let Some(author_id) = upsert_user_if_present(
                        db.clone(),
                        account_id,
                        &author,
                        parse_timestamp(&comment.updated_at),
                    )
                    .await?
                    {
                        db.upsert_comment(&CommentRecord {
                            id: comment.id,
                            account_id: account_id.to_string(),
                            pr_id: pr.id.clone(),
                            kind: "issue".to_string(),
                            author_id,
                            body: comment.body,
                            created_at: parse_timestamp(&comment.created_at),
                            updated_at: parse_timestamp(&comment.updated_at),
                            deleted_at: None,
                            in_reply_to_id: None,
                            review_id: None,
                            thread_id: None,
                            path: None,
                            line: None,
                            side: None,
                            start_line: None,
                            start_side: None,
                            original_commit_sha: None,
                        })
                        .await?;
                    }
                }
            }
            TimelineItem::PullRequestReview(review) => {
                review_map.entry(review.id.clone()).or_insert(review);
            }
        }
    }

    for review in review_map.values() {
        let Some(author) = review.author.as_ref() else {
            continue;
        };
        let Some(author_id) = upsert_user_if_present(
            db.clone(),
            account_id,
            author,
            parse_timestamp(&review.updated_at),
        )
        .await?
        else {
            continue;
        };
        db.upsert_review(&ReviewRecord {
            id: review.id.clone(),
            account_id: account_id.to_string(),
            pr_id: pr.id.clone(),
            author_id,
            state: review.state.clone(),
            body: review.body.clone().unwrap_or_default(),
            commit_sha: review.commit.as_ref().map(|commit| commit.oid.clone()),
            submitted_at: review.submitted_at.as_deref().map(parse_timestamp),
            created_at: parse_timestamp(&review.created_at),
            updated_at: parse_timestamp(&review.updated_at),
        })
        .await?;
    }

    for thread in pr
        .review_threads
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
    {
        let resolved_by_id = if let Some(resolved_by) = &thread.resolved_by {
            upsert_user_if_present(
                db.clone(),
                account_id,
                resolved_by,
                parse_timestamp(&pr.updated_at),
            )
            .await?
        } else {
            None
        };
        db.upsert_review_thread(&ReviewThreadRecord {
            id: thread.id.clone(),
            account_id: account_id.to_string(),
            pr_id: pr.id.clone(),
            path: thread.path.clone(),
            line: thread.line,
            side: thread.side.clone(),
            start_line: thread.start_line,
            start_side: thread.start_side.clone(),
            original_commit_sha: thread
                .original_commit
                .as_ref()
                .map(|value| value.oid.clone()),
            original_path: Some(thread.path.clone()),
            original_position: thread.original_start_line,
            original_line: thread.original_line,
            is_outdated: thread.is_outdated,
            is_resolved: thread.is_resolved,
            resolved_by_id,
            created_at: parse_timestamp(&pr.created_at),
            updated_at: parse_timestamp(&pr.updated_at),
        })
        .await?;

        for comment in thread
            .comments
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            let Some(author) = comment.author.as_ref() else {
                continue;
            };
            let Some(author_id) = upsert_user_if_present(
                db.clone(),
                account_id,
                author,
                parse_timestamp(&comment.updated_at),
            )
            .await?
            else {
                continue;
            };
            db.upsert_comment(&CommentRecord {
                id: comment.id.clone(),
                account_id: account_id.to_string(),
                pr_id: pr.id.clone(),
                kind: "review_thread_reply".to_string(),
                author_id,
                body: comment.body.clone(),
                created_at: parse_timestamp(&comment.created_at),
                updated_at: parse_timestamp(&comment.updated_at),
                deleted_at: None,
                in_reply_to_id: None,
                review_id: None,
                thread_id: Some(thread.id.clone()),
                path: Some(thread.path.clone()),
                line: thread.line,
                side: thread.side.clone(),
                start_line: thread.start_line,
                start_side: thread.start_side.clone(),
                original_commit_sha: thread
                    .original_commit
                    .as_ref()
                    .map(|value| value.oid.clone()),
            })
            .await?;
        }
    }

    for commit in pr.commits.nodes.unwrap_or_default().into_iter().flatten() {
        let Some(check_suites) = commit.commit.check_suites else {
            continue;
        };
        for suite in check_suites.nodes.unwrap_or_default().into_iter().flatten() {
            let details_url = suite.workflow_run.and_then(|workflow| workflow.url);
            db.upsert_check_suite(&CheckSuiteRecord {
                id: suite.id.clone(),
                account_id: account_id.to_string(),
                pr_id: pr.id.clone(),
                head_sha: pr.head_ref_oid.clone(),
                app_name: suite
                    .app
                    .as_ref()
                    .map(|app| app.name.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                status: suite.status.unwrap_or_else(|| "unknown".to_string()),
                conclusion: suite.conclusion.clone(),
                details_url,
                created_at: parse_timestamp(&pr.created_at),
                updated_at: parse_timestamp(&pr.updated_at),
            })
            .await?;

            for run in suite
                .check_runs
                .as_ref()
                .and_then(|runs| runs.nodes.as_ref())
                .into_iter()
                .flatten()
                .flatten()
            {
                db.upsert_check_run(&CheckRunRecord {
                    id: run.id.clone(),
                    account_id: account_id.to_string(),
                    check_suite_id: suite.id.clone(),
                    pr_id: pr.id.clone(),
                    name: run.name.clone(),
                    status: run.status.clone().unwrap_or_else(|| "unknown".to_string()),
                    conclusion: run.conclusion.clone(),
                    details_url: run.details_url.clone(),
                    output_title: run.title.clone(),
                    output_summary: run.summary.clone(),
                    started_at: run.started_at.as_deref().map(parse_timestamp),
                    completed_at: run.completed_at.as_deref().map(parse_timestamp),
                    created_at: parse_timestamp(&pr.created_at),
                    updated_at: parse_timestamp(&pr.updated_at),
                })
                .await?;
            }
        }
    }

    Ok(Some(RefetchTarget {
        owner: repository.owner.login,
        repo: repository.name,
        number: pr.number,
    }))
}

fn build_push_record(
    pr_id: &str,
    account_id: &str,
    base_sha: &str,
    head_sha: &str,
    observed_at: i64,
    previous_push: Option<&crate::db::PrPushRow>,
    commit_oids: &[String],
) -> Option<PrPushRecord> {
    let (push_kind, supersedes_head_sha) = match previous_push {
        None => ("initial".to_string(), None),
        Some(previous) => {
            if previous.head_sha == head_sha {
                return None;
            }
            let old_in_new_range = commit_oids.iter().any(|oid| oid == &previous.head_sha);
            let kind = if old_in_new_range {
                if previous.base_sha != base_sha {
                    "merge-back"
                } else {
                    "fast-forward"
                }
            } else {
                "force-push"
            };
            (kind.to_string(), Some(previous.head_sha.clone()))
        }
    };
    Some(PrPushRecord {
        pr_id: pr_id.to_string(),
        account_id: account_id.to_string(),
        head_sha: head_sha.to_string(),
        base_sha: base_sha.to_string(),
        observed_at,
        push_kind,
        supersedes_head_sha,
    })
}

pub async fn reconcile_inbox_refresh(
    db: Arc<Db>,
    account_id: &str,
    data: InboxRefreshData,
) -> Result<Vec<RefetchTarget>> {
    let mut targets = Vec::new();
    for node in data.nodes.into_iter().flatten() {
        if node.typename != "PullRequest" {
            continue;
        }
        let Some(pr_id) = node.id.clone() else {
            continue;
        };
        let Some(repo) = node.repository.as_ref() else {
            continue;
        };
        let now = now_epoch_seconds()?;
        db.upsert_repo(&RepoRecord {
            id: repo.id.clone(),
            account_id: account_id.to_string(),
            owner: repo.owner.login.clone(),
            name: repo.name.clone(),
            default_branch: None,
            description: None,
            html_url: None,
            is_private: false,
            is_archived: false,
            pushed_at: None,
            created_at: now,
            updated_at: node
                .updated_at
                .as_deref()
                .map(parse_timestamp)
                .unwrap_or(now),
        })
        .await?;

        let author_id = if let Some(author) = &node.author {
            upsert_user_if_present(
                db.clone(),
                account_id,
                author,
                node.updated_at
                    .as_deref()
                    .map(parse_timestamp)
                    .unwrap_or(now),
            )
            .await?
        } else {
            None
        };

        let check_state = node
            .commits
            .as_ref()
            .and_then(|commits| commits.nodes.as_ref())
            .and_then(|nodes| nodes.first())
            .and_then(|entry| entry.as_ref())
            .and_then(|entry| entry.commit.status_check_rollup.as_ref())
            .and_then(|rollup| rollup.state.clone());

        db.upsert_pull_request(&PullRequestRecord {
            id: pr_id.clone(),
            account_id: account_id.to_string(),
            repo_id: repo.id.clone(),
            number: node.number.unwrap_or_default(),
            state: node
                .state
                .clone()
                .unwrap_or_else(|| "OPEN".to_string())
                .to_lowercase(),
            draft: node.is_draft.unwrap_or(false),
            title: node.title.clone().unwrap_or_default(),
            body: String::new(),
            author_id,
            base_ref: String::new(),
            base_sha: node.base_ref_oid.clone().unwrap_or_default(),
            head_ref: String::new(),
            head_sha: node.head_ref_oid.clone().unwrap_or_default(),
            head_repo_id: None,
            mergeable_state: node.mergeable.clone(),
            merge_state_status: check_state,
            merge_commit_allowed: None,
            squash_merge_allowed: None,
            rebase_merge_allowed: None,
            delete_branch_on_merge_default: None,
            viewer_can_merge: None,
            viewer_can_enable_auto_merge: None,
            viewer_can_disable_auto_merge: None,
            viewer_can_update_branch: None,
            viewer_can_delete_head_ref: None,
            auto_merge_enabled: None,
            auto_merge_method: None,
            auto_merge_commit_headline: None,
            auto_merge_commit_body: None,
            auto_merge_enabled_by_login: None,
            auto_merge_enabled_at: None,
            merge_queue_entry_id: None,
            merge_queue_entry_position: None,
            merge_queue_entry_state: None,
            merge_queue_entry_estimated_ms: None,
            branch_protection_summary_json: None,
            repo_has_merge_queue: None,
            head_ref_state: None,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            comments_count: 0,
            reviews_count: 0,
            commits_count: 0,
            is_read: !node.unread_placeholder.unwrap_or(false),
            html_url: None,
            created_at: now,
            updated_at: node
                .updated_at
                .as_deref()
                .map(parse_timestamp)
                .unwrap_or(now),
            closed_at: None,
            merged_at: None,
        })
        .await?;

        let labels = node
            .labels
            .as_ref()
            .and_then(|labels| labels.nodes.as_ref())
            .into_iter()
            .flatten()
            .flatten()
            .map(|label| PrLabelRecord {
                account_id: account_id.to_string(),
                pr_id: pr_id.clone(),
                label_name: label.name.clone(),
                label_color: label.color.clone(),
                description: label.description.clone(),
            })
            .collect::<Vec<_>>();
        db.replace_pr_labels(account_id, &pr_id, &labels).await?;

        let mut reviewers = Vec::new();
        for request in node
            .review_requests
            .as_ref()
            .and_then(|requests| requests.nodes.as_ref())
            .into_iter()
            .flatten()
            .flatten()
        {
            let Some(requested) = request.requested_reviewer.as_ref() else {
                continue;
            };
            match requested {
                RequestedReviewer::User(user) => {
                    if let Some(user_id) =
                        upsert_user_if_present(db.clone(), account_id, user, now).await?
                    {
                        reviewers.push(PrReviewerRecord {
                            account_id: account_id.to_string(),
                            pr_id: pr_id.clone(),
                            user_id,
                            reviewer_type: "user".to_string(),
                            reviewer_state: "requested".to_string(),
                            requested_at: now,
                        });
                    }
                }
                RequestedReviewer::Team(team) => {
                    reviewers.push(PrReviewerRecord {
                        account_id: account_id.to_string(),
                        pr_id: pr_id.clone(),
                        user_id: team.id.clone(),
                        reviewer_type: "team".to_string(),
                        reviewer_state: "requested".to_string(),
                        requested_at: now,
                    });
                }
            }
        }
        db.replace_pr_reviewers(account_id, &pr_id, &reviewers)
            .await?;

        targets.push(RefetchTarget {
            owner: repo.owner.login.clone(),
            repo: repo.name.clone(),
            number: node.number.unwrap_or_default(),
        });
    }
    Ok(targets)
}

pub async fn reconcile_notifications(
    db: Arc<Db>,
    account_id: &str,
    notifications: Vec<NotificationPayload>,
) -> Result<Vec<RefetchTarget>> {
    let mut targets = Vec::new();
    for notification in notifications {
        let updated_at = parse_timestamp(&notification.updated_at);
        db.upsert_repo(&RepoRecord {
            id: notification.repository_id.clone(),
            account_id: account_id.to_string(),
            owner: notification.repository_owner.clone(),
            name: notification.repository_name.clone(),
            default_branch: None,
            description: notification.repository_description.clone(),
            html_url: notification.repository_html_url.clone(),
            is_private: notification.repository_private,
            is_archived: notification.repository_archived,
            pushed_at: None,
            created_at: updated_at,
            updated_at,
        })
        .await?;

        let subject_id = notification
            .subject_url
            .clone()
            .unwrap_or_else(|| notification.subject_title.clone());
        let pr_number = parse_pr_number_from_subject(notification.subject_url.as_deref());
        let pr_id = if let Some(number) = pr_number {
            sqlx::query_scalar::<_, String>(
                "SELECT pr.id
                 FROM pull_requests pr
                 JOIN repos r ON r.id = pr.repo_id
                 WHERE pr.account_id = ?1
                   AND r.owner = ?2
                   AND r.name = ?3
                   AND pr.number = ?4
                 LIMIT 1",
            )
            .bind(account_id)
            .bind(&notification.repository_owner)
            .bind(&notification.repository_name)
            .bind(number)
            .fetch_optional(db.pool())
            .await?
        } else {
            None
        };

        db.upsert_notification(&NotificationRecord {
            id: notification.id.clone(),
            account_id: account_id.to_string(),
            repo_id: notification.repository_id.clone(),
            pr_id: pr_id.clone(),
            reason: notification.reason.clone(),
            subject_type: notification.subject_type.clone(),
            subject_id,
            title: notification.subject_title.clone(),
            unread: notification.unread,
            updated_at,
            last_read_at: notification.last_read_at.as_deref().map(parse_timestamp),
            url: notification.url.clone(),
        })
        .await?;

        if let Some(number) = pr_number {
            targets.push(RefetchTarget {
                owner: notification.repository_owner.clone(),
                repo: notification.repository_name.clone(),
                number,
            });
        }
    }
    Ok(targets)
}

async fn upsert_user_if_present(
    db: Arc<Db>,
    account_id: &str,
    user: &GraphqlUser,
    updated_at: i64,
) -> Result<Option<String>> {
    let Some(id) = user.id.clone() else {
        return Ok(None);
    };
    db.upsert_user(&UserRecord {
        id: id.clone(),
        account_id: account_id.to_string(),
        login: user.login.clone().unwrap_or_else(|| "unknown".to_string()),
        display_name: user.name.clone(),
        avatar_url: user.avatar_url.clone(),
        html_url: user.url.clone(),
        created_at: updated_at,
        updated_at,
    })
    .await?;
    Ok(Some(id))
}

pub fn parse_timestamp(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.timestamp())
        .unwrap_or_default()
}

fn parse_pr_number_from_subject(subject_url: Option<&str>) -> Option<i64> {
    let url = subject_url?;
    let needle = "/pulls/";
    let index = url.find(needle)?;
    let number_part = &url[index + needle.len()..];
    let number = number_part
        .split('/')
        .next()
        .ok_or_else(|| anyhow!("missing pull number"))
        .ok()?;
    number.parse::<i64>().ok()
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(i64::try_from(elapsed.as_secs())?)
}
