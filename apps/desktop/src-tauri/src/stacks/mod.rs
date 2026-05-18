pub mod graphite;
pub mod ops;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::Row;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PullRequestRow {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub state: String,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
    pub merge_state_status: Option<String>,
    pub review_decision: Option<String>,
    pub check_rollup_state: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct BlockedByReason {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StackKind {
    Linear,
    Dag,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct StackWarning {
    pub reason: String,
    pub diamond_pr_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct StackNode {
    pub pr_id: String,
    pub pr_number: i64,
    pub title: String,
    pub state: String,
    pub position: i64,
    pub parent_pr_id: Option<String>,
    pub blocked_by: Vec<BlockedByReason>,
    pub review_decision: Option<String>,
    pub check_rollup_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub base_ref: String,
    pub head_ref: String,
    pub base_sha: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct StackEdge {
    pub from_pr_id: String,
    pub to_pr_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct StackGraph {
    pub stack_id: String,
    pub account_id: String,
    pub repo_id: String,
    pub kind: StackKind,
    pub nodes: Vec<StackNode>,
    pub edges: Vec<StackEdge>,
    pub warning: Option<StackWarning>,
    pub head_pr_id: Option<String>,
    pub base_branch: Option<String>,
    pub detected_at: i64,
    pub updated_at: i64,
}

pub fn stacks_changed_event_name(account_id: &str, repo_id: &str) -> String {
    format!("stacks:{account_id}:{repo_id} changed")
}

pub fn compute_blocked_by(
    pr: &PullRequestRow,
    parent: Option<&PullRequestRow>,
) -> Vec<BlockedByReason> {
    let mut reasons = Vec::new();
    let review = pr
        .review_decision
        .clone()
        .unwrap_or_else(|| "REVIEW_REQUIRED".to_string());
    if !review.eq_ignore_ascii_case("approved") {
        reasons.push(BlockedByReason {
            kind: "awaiting_review".to_string(),
            detail: format!("reviewDecision={review}"),
        });
    }

    if let Some(ci_state) = pr.check_rollup_state.as_deref() {
        if ci_state.eq_ignore_ascii_case("failure") || ci_state.eq_ignore_ascii_case("error") {
            reasons.push(BlockedByReason {
                kind: "ci_failing".to_string(),
                detail: format!("checkRunRollup.state={ci_state}"),
            });
        } else if ci_state.eq_ignore_ascii_case("pending") {
            reasons.push(BlockedByReason {
                kind: "ci_pending".to_string(),
                detail: format!("checkRunRollup.state={ci_state}"),
            });
        }
    }

    if let Some(merge_state_status) = pr.merge_state_status.as_deref() {
        let upper = merge_state_status.to_ascii_uppercase();
        if matches!(upper.as_str(), "DIRTY" | "CONFLICTING" | "UNKNOWN") {
            reasons.push(BlockedByReason {
                kind: "conflict".to_string(),
                detail: format!("merge_state_status={merge_state_status}"),
            });
        }
    }

    if let Some(parent_pr) = parent {
        if parent_pr.state.eq_ignore_ascii_case("open") {
            reasons.push(BlockedByReason {
                kind: "parent_unmerged".to_string(),
                detail: format!("parent PR #{} is still open", parent_pr.number),
            });
        }
    }

    reasons
}

pub fn detect_stacks(prs: &[PullRequestRow], repo_id: &str, account_id: &str) -> Vec<StackGraph> {
    if prs.is_empty() {
        return Vec::new();
    }

    let now = now_epoch_seconds().unwrap_or(0);
    let mut by_id = HashMap::<String, PullRequestRow>::new();
    let mut by_head_ref = HashMap::<String, Vec<String>>::new();
    for pr in prs {
        by_head_ref
            .entry(pr.head_ref.clone())
            .or_default()
            .push(pr.id.clone());
        by_id.insert(pr.id.clone(), pr.clone());
    }

    let mut outgoing = HashMap::<String, BTreeSet<String>>::new();
    let mut incoming = HashMap::<String, BTreeSet<String>>::new();
    for pr in prs {
        outgoing.entry(pr.id.clone()).or_default();
        incoming.entry(pr.id.clone()).or_default();
    }

    for child in prs {
        if let Some(parents) = by_head_ref.get(&child.base_ref) {
            for parent_id in parents {
                if parent_id == &child.id {
                    continue;
                }
                outgoing
                    .entry(parent_id.clone())
                    .or_default()
                    .insert(child.id.clone());
                incoming
                    .entry(child.id.clone())
                    .or_default()
                    .insert(parent_id.clone());
            }
        }
    }

    let mut visited = BTreeSet::<String>::new();
    let mut components = Vec::<Vec<String>>::new();

    for pr in prs {
        if visited.contains(&pr.id) {
            continue;
        }
        let mut queue = VecDeque::from([pr.id.clone()]);
        let mut component = BTreeSet::new();
        while let Some(node) = queue.pop_front() {
            if !component.insert(node.clone()) {
                continue;
            }
            if let Some(children) = outgoing.get(&node) {
                for child in children {
                    queue.push_back(child.clone());
                }
            }
            if let Some(parents) = incoming.get(&node) {
                for parent in parents {
                    queue.push_back(parent.clone());
                }
            }
        }
        for node in &component {
            visited.insert(node.clone());
        }
        components.push(component.into_iter().collect());
    }

    components
        .into_iter()
        .filter_map(|component| {
            if component.is_empty() {
                return None;
            }

            let mut component_edges = Vec::<(String, String)>::new();
            let mut indegree = HashMap::<String, usize>::new();
            let mut outdegree = HashMap::<String, usize>::new();
            for id in &component {
                indegree.insert(id.clone(), 0);
                outdegree.insert(id.clone(), 0);
            }
            let component_set: BTreeSet<String> = component.iter().cloned().collect();
            for from in &component {
                for to in outgoing.get(from).into_iter().flatten() {
                    if component_set.contains(to) {
                        component_edges.push((from.clone(), to.clone()));
                        *outdegree.entry(from.clone()).or_default() += 1;
                        *indegree.entry(to.clone()).or_default() += 1;
                    }
                }
            }

            let has_branching = component.iter().any(|id| {
                outdegree.get(id).copied().unwrap_or(0) > 1
                    || indegree.get(id).copied().unwrap_or(0) > 1
            });
            let mut indegree_work = indegree.clone();
            let mut topo_queue = component
                .iter()
                .filter(|id| indegree_work.get(*id).copied().unwrap_or(0) == 0)
                .cloned()
                .collect::<Vec<_>>();
            topo_queue.sort();
            let mut topo = Vec::new();
            let mut depth = HashMap::<String, i64>::new();
            for root in &topo_queue {
                depth.insert(root.clone(), 0);
            }

            let mut topo_queue = VecDeque::from(topo_queue);
            while let Some(node) = topo_queue.pop_front() {
                topo.push(node.clone());
                let parent_depth = depth.get(&node).copied().unwrap_or(0);
                for child in outgoing.get(&node).into_iter().flatten() {
                    if !component_set.contains(child) {
                        continue;
                    }
                    let child_depth = depth.get(child).copied().unwrap_or(0);
                    if parent_depth + 1 > child_depth {
                        depth.insert(child.clone(), parent_depth + 1);
                    }
                    if let Some(value) = indegree_work.get_mut(child) {
                        *value = value.saturating_sub(1);
                        if *value == 0 {
                            topo_queue.push_back(child.clone());
                        }
                    }
                }
            }

            let has_cycle = topo.len() != component.len();
            if has_cycle {
                for id in &component {
                    depth.entry(id.clone()).or_insert(0);
                }
                for _ in 0..component.len() {
                    for (from, to) in &component_edges {
                        let parent_depth = depth.get(from).copied().unwrap_or(0);
                        let child_depth = depth.get(to).copied().unwrap_or(0);
                        if parent_depth + 1 > child_depth {
                            depth.insert(to.clone(), parent_depth + 1);
                        }
                    }
                }
            }

            let kind = if has_branching || has_cycle {
                StackKind::Dag
            } else {
                StackKind::Linear
            };

            let mut cycle_nodes = BTreeSet::<String>::new();
            if has_cycle {
                for (node, remaining) in indegree_work {
                    if remaining > 0 {
                        cycle_nodes.insert(node);
                    }
                }
            }
            let mut diamond_nodes = component
                .iter()
                .filter(|id| {
                    outdegree.get(*id).copied().unwrap_or(0) > 1
                        || indegree.get(*id).copied().unwrap_or(0) > 1
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            diamond_nodes.extend(cycle_nodes);
            let warning = match kind {
                StackKind::Linear => None,
                StackKind::Dag => Some(StackWarning {
                    reason: if has_cycle {
                        "cycle detected".to_string()
                    } else {
                        "diamond detected".to_string()
                    },
                    diamond_pr_ids: diamond_nodes.into_iter().collect(),
                }),
            };

            let mut parent_lookup = HashMap::<String, Option<String>>::new();
            for id in &component {
                let mut parents = incoming
                    .get(id)
                    .into_iter()
                    .flatten()
                    .filter(|parent| component_set.contains(*parent))
                    .cloned()
                    .collect::<Vec<_>>();
                parents.sort_by(|left, right| {
                    let ld = depth.get(left).copied().unwrap_or(0);
                    let rd = depth.get(right).copied().unwrap_or(0);
                    rd.cmp(&ld).then_with(|| left.cmp(right))
                });
                parent_lookup.insert(id.clone(), parents.first().cloned());
            }

            let mut rows = component
                .iter()
                .filter_map(|id| by_id.get(id).cloned())
                .collect::<Vec<_>>();
            rows.sort_by(|left, right| {
                let l_pos = depth.get(&left.id).copied().unwrap_or(0);
                let r_pos = depth.get(&right.id).copied().unwrap_or(0);
                l_pos
                    .cmp(&r_pos)
                    .then_with(|| left.number.cmp(&right.number))
                    .then_with(|| left.id.cmp(&right.id))
            });

            let mut nodes = Vec::with_capacity(rows.len());
            for pr in &rows {
                let parent = parent_lookup
                    .get(&pr.id)
                    .and_then(|parent_id| parent_id.as_ref())
                    .and_then(|parent_id| by_id.get(parent_id));
                nodes.push(StackNode {
                    pr_id: pr.id.clone(),
                    pr_number: pr.number,
                    title: pr.title.clone(),
                    state: pr.state.clone(),
                    position: depth.get(&pr.id).copied().unwrap_or(0),
                    parent_pr_id: parent.map(|value| value.id.clone()),
                    blocked_by: compute_blocked_by(pr, parent),
                    review_decision: pr.review_decision.clone(),
                    check_rollup_state: pr.check_rollup_state.clone(),
                    merge_state_status: pr.merge_state_status.clone(),
                    base_ref: pr.base_ref.clone(),
                    head_ref: pr.head_ref.clone(),
                    base_sha: pr.base_sha.clone(),
                    head_sha: pr.head_sha.clone(),
                });
            }

            let mut edges = component_edges
                .into_iter()
                .map(|(from_pr_id, to_pr_id)| StackEdge {
                    from_pr_id,
                    to_pr_id,
                })
                .collect::<Vec<_>>();
            edges.sort_by(|left, right| {
                left.from_pr_id
                    .cmp(&right.from_pr_id)
                    .then_with(|| left.to_pr_id.cmp(&right.to_pr_id))
            });

            let mut roots = nodes
                .iter()
                .filter(|node| node.parent_pr_id.is_none())
                .collect::<Vec<_>>();
            roots.sort_by(|left, right| {
                left.position
                    .cmp(&right.position)
                    .then_with(|| left.pr_number.cmp(&right.pr_number))
            });
            let base_branch = roots.first().map(|node| node.base_ref.clone());
            let head_pr_id = match kind {
                StackKind::Linear => roots.first().map(|node| node.pr_id.clone()),
                StackKind::Dag => {
                    let max_depth = nodes.iter().map(|node| node.position).max().unwrap_or(0);
                    let mut deepest = nodes
                        .iter()
                        .filter(|node| node.position == max_depth)
                        .map(|node| node.pr_id.clone())
                        .collect::<Vec<_>>();
                    deepest.sort();
                    if deepest.len() == 1 {
                        deepest.first().cloned()
                    } else {
                        None
                    }
                }
            };

            Some(StackGraph {
                stack_id: stack_id_for_component(account_id, repo_id, &component),
                account_id: account_id.to_string(),
                repo_id: repo_id.to_string(),
                kind,
                nodes,
                edges,
                warning,
                head_pr_id,
                base_branch,
                detected_at: now,
                updated_at: now,
            })
        })
        .collect()
}

pub async fn list_open_pull_requests(
    db: &Db,
    account_id: &str,
    repo_id: &str,
) -> Result<Vec<PullRequestRow>> {
    let rows = sqlx::query(
        "SELECT
            pr.id,
            pr.account_id,
            pr.repo_id,
            pr.number,
            pr.title,
            pr.state,
            pr.base_ref,
            pr.base_sha,
            pr.head_ref,
            pr.head_sha,
            pr.merge_state_status,
            review_summary.review_decision,
            check_summary.rollup_state AS check_rollup_state,
            pr.updated_at
         FROM pull_requests pr
         LEFT JOIN (
            SELECT
              account_id,
              pr_id,
              CASE
                WHEN SUM(CASE WHEN UPPER(state) = 'CHANGES_REQUESTED' THEN 1 ELSE 0 END) > 0 THEN 'CHANGES_REQUESTED'
                WHEN SUM(CASE WHEN UPPER(state) = 'APPROVED' THEN 1 ELSE 0 END) > 0 THEN 'APPROVED'
                WHEN COUNT(*) > 0 THEN 'REVIEWED'
                ELSE 'REVIEW_REQUIRED'
              END AS review_decision
            FROM reviews
            GROUP BY account_id, pr_id
         ) review_summary
           ON review_summary.account_id = pr.account_id
          AND review_summary.pr_id = pr.id
         LEFT JOIN (
            SELECT
              account_id,
              pr_id,
              CASE
                WHEN SUM(CASE WHEN LOWER(status) IN ('queued','in_progress','waiting','pending','requested') THEN 1 ELSE 0 END) > 0 THEN 'PENDING'
                WHEN SUM(CASE WHEN LOWER(COALESCE(conclusion, '')) IN ('failure','failed','timed_out','cancelled','action_required','startup_failure') THEN 1 ELSE 0 END) > 0 THEN 'FAILURE'
                WHEN COUNT(*) = 0 THEN NULL
                ELSE 'SUCCESS'
              END AS rollup_state
            FROM check_runs
            GROUP BY account_id, pr_id
         ) check_summary
           ON check_summary.account_id = pr.account_id
          AND check_summary.pr_id = pr.id
         WHERE pr.account_id = ?1
           AND pr.repo_id = ?2
           AND pr.state = 'open'
         ORDER BY pr.number ASC",
    )
    .bind(account_id)
    .bind(repo_id)
    .fetch_all(db.pool())
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(PullRequestRow {
                id: row.try_get("id")?,
                account_id: row.try_get("account_id")?,
                repo_id: row.try_get("repo_id")?,
                number: row.try_get("number")?,
                title: row.try_get("title")?,
                state: row.try_get("state")?,
                base_ref: row.try_get("base_ref")?,
                base_sha: row.try_get("base_sha")?,
                head_ref: row.try_get("head_ref")?,
                head_sha: row.try_get("head_sha")?,
                merge_state_status: row.try_get("merge_state_status")?,
                review_decision: row.try_get("review_decision")?,
                check_rollup_state: row.try_get("check_rollup_state")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

pub async fn upsert_stack_state(db: &Db, stacks: &[StackGraph]) -> Result<()> {
    if stacks.is_empty() {
        return Ok(());
    }
    let account_id = &stacks[0].account_id;
    let repo_id = &stacks[0].repo_id;
    upsert_stack_state_for_scope(db, account_id, repo_id, stacks).await
}

pub async fn upsert_stack_state_for_scope(
    db: &Db,
    account_id: &str,
    repo_id: &str,
    stacks: &[StackGraph],
) -> Result<()> {
    let now = now_epoch_seconds().unwrap_or(0);
    let mut tx = db.pool().begin().await?;
    sqlx::query("DELETE FROM stacks WHERE account_id = ?1 AND repo_id = ?2")
        .bind(account_id)
        .bind(repo_id)
        .execute(tx.as_mut())
        .await?;

    for stack in stacks {
        let kind = match stack.kind {
            StackKind::Linear => "linear",
            StackKind::Dag => "dag",
        };
        let warning_json = stack
            .warning
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serializing stack warning")?;
        sqlx::query(
            "INSERT INTO stacks(
                id, repo_id, account_id, kind, warning_json, head_pr_id, base_branch, detected_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&stack.stack_id)
        .bind(&stack.repo_id)
        .bind(&stack.account_id)
        .bind(kind)
        .bind(warning_json)
        .bind(&stack.head_pr_id)
        .bind(&stack.base_branch)
        .bind(stack.detected_at)
        .bind(stack.updated_at.max(now))
        .execute(tx.as_mut())
        .await?;

        for node in &stack.nodes {
            let blocked_by_json = serde_json::to_string(&node.blocked_by)
                .context("serializing blocked_by reasons")?;
            sqlx::query(
                "INSERT INTO pr_stack_position(
                    pr_id, stack_id, position, parent_pr_id, blocked_by_json, computed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&node.pr_id)
            .bind(&stack.stack_id)
            .bind(node.position)
            .bind(&node.parent_pr_id)
            .bind(blocked_by_json)
            .bind(now)
            .execute(tx.as_mut())
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

pub async fn list_stacks(db: &Db, account_id: &str, repo_id: &str) -> Result<Vec<StackGraph>> {
    let rows = sqlx::query(
        "SELECT
            summary.stack_id,
            summary.stack_kind,
            summary.stack_warning_json,
            summary.stack_head_pr_id,
            summary.stack_base_branch,
            summary.stack_detected_at,
            summary.stack_updated_at,
            summary.pr_id,
            summary.pr_number,
            summary.title,
            summary.state,
            summary.stack_position,
            summary.parent_pr_id,
            summary.blocked_by_json,
            summary.merge_state_status,
            summary.base_sha,
            summary.head_sha,
            pr.base_ref,
            pr.head_ref,
            review_summary.review_decision,
            check_summary.rollup_state AS check_rollup_state
         FROM pr_stack_summary summary
         JOIN pull_requests pr ON pr.account_id = summary.account_id AND pr.id = summary.pr_id
         LEFT JOIN (
            SELECT
              account_id,
              pr_id,
              CASE
                WHEN SUM(CASE WHEN UPPER(state) = 'CHANGES_REQUESTED' THEN 1 ELSE 0 END) > 0 THEN 'CHANGES_REQUESTED'
                WHEN SUM(CASE WHEN UPPER(state) = 'APPROVED' THEN 1 ELSE 0 END) > 0 THEN 'APPROVED'
                WHEN COUNT(*) > 0 THEN 'REVIEWED'
                ELSE 'REVIEW_REQUIRED'
              END AS review_decision
            FROM reviews
            GROUP BY account_id, pr_id
         ) review_summary
           ON review_summary.account_id = summary.account_id
          AND review_summary.pr_id = summary.pr_id
         LEFT JOIN (
            SELECT
              account_id,
              pr_id,
              CASE
                WHEN SUM(CASE WHEN LOWER(status) IN ('queued','in_progress','waiting','pending','requested') THEN 1 ELSE 0 END) > 0 THEN 'PENDING'
                WHEN SUM(CASE WHEN LOWER(COALESCE(conclusion, '')) IN ('failure','failed','timed_out','cancelled','action_required','startup_failure') THEN 1 ELSE 0 END) > 0 THEN 'FAILURE'
                WHEN COUNT(*) = 0 THEN NULL
                ELSE 'SUCCESS'
              END AS rollup_state
            FROM check_runs
            GROUP BY account_id, pr_id
         ) check_summary
           ON check_summary.account_id = summary.account_id
          AND check_summary.pr_id = summary.pr_id
         WHERE summary.account_id = ?1
           AND summary.repo_id = ?2
         ORDER BY summary.stack_id ASC, summary.stack_position ASC, summary.pr_number ASC",
    )
    .bind(account_id)
    .bind(repo_id)
    .fetch_all(db.pool())
    .await?;

    let mut grouped = BTreeMap::<String, StackGraph>::new();
    for row in rows {
        let stack_id: String = row.try_get("stack_id")?;
        let stack_kind_raw: String = row.try_get("stack_kind")?;
        let stack_kind = if stack_kind_raw.eq_ignore_ascii_case("linear") {
            StackKind::Linear
        } else {
            StackKind::Dag
        };
        let warning = row
            .try_get::<Option<String>, _>("stack_warning_json")?
            .map(|value| serde_json::from_str::<StackWarning>(&value))
            .transpose()
            .context("parsing stack warning json")?;

        let entry = grouped
            .entry(stack_id.clone())
            .or_insert_with(|| StackGraph {
                stack_id: stack_id.clone(),
                account_id: account_id.to_string(),
                repo_id: repo_id.to_string(),
                kind: stack_kind,
                nodes: Vec::new(),
                edges: Vec::new(),
                warning: warning.clone(),
                head_pr_id: row.try_get("stack_head_pr_id").ok().flatten(),
                base_branch: row.try_get("stack_base_branch").ok().flatten(),
                detected_at: row.try_get("stack_detected_at").unwrap_or_default(),
                updated_at: row.try_get("stack_updated_at").unwrap_or_default(),
            });

        let blocked_by = row
            .try_get::<Option<String>, _>("blocked_by_json")?
            .map(|value| serde_json::from_str::<Vec<BlockedByReason>>(&value))
            .transpose()
            .context("parsing blocked_by json")?
            .unwrap_or_default();

        let node = StackNode {
            pr_id: row.try_get("pr_id")?,
            pr_number: row.try_get("pr_number")?,
            title: row.try_get("title")?,
            state: row.try_get("state")?,
            position: row.try_get("stack_position")?,
            parent_pr_id: row.try_get("parent_pr_id")?,
            blocked_by,
            review_decision: row.try_get("review_decision")?,
            check_rollup_state: row.try_get("check_rollup_state")?,
            merge_state_status: row.try_get("merge_state_status")?,
            base_ref: row.try_get("base_ref")?,
            head_ref: row.try_get("head_ref")?,
            base_sha: row.try_get("base_sha")?,
            head_sha: row.try_get("head_sha")?,
        };
        if let Some(parent_id) = node.parent_pr_id.clone() {
            entry.edges.push(StackEdge {
                from_pr_id: parent_id,
                to_pr_id: node.pr_id.clone(),
            });
        }
        entry.nodes.push(node);
    }

    Ok(grouped.into_values().collect())
}

fn stack_id_for_component(account_id: &str, repo_id: &str, component: &[String]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    account_id.hash(&mut hasher);
    repo_id.hash(&mut hasher);
    let mut sorted = component.to_vec();
    sorted.sort();
    for id in sorted {
        id.hash(&mut hasher);
    }
    format!("stack-{:016x}", hasher.finish())
}

pub fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}
