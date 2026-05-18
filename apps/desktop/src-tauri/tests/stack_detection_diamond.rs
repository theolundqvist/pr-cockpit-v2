use desktop_lib::stacks::{detect_stacks, PullRequestRow, StackKind};

fn pr(id: &str, number: i64, base_ref: &str, head_ref: &str) -> PullRequestRow {
    PullRequestRow {
        id: id.to_string(),
        account_id: "acct_stack".to_string(),
        repo_id: "repo_stack".to_string(),
        number,
        title: format!("PR {number}"),
        state: "open".to_string(),
        base_ref: base_ref.to_string(),
        base_sha: format!("base-{id}"),
        head_ref: head_ref.to_string(),
        head_sha: format!("head-{id}"),
        merge_state_status: Some("CLEAN".to_string()),
        review_decision: Some("REVIEW_REQUIRED".to_string()),
        check_rollup_state: Some("PENDING".to_string()),
        updated_at: 1,
    }
}

#[test]
fn detects_dag_diamond_topology() {
    let prs = vec![
        pr("pr_r", 100, "main", "stack/root"),
        pr("pr_ml", 101, "stack/root", "stack/shared-mid"),
        pr("pr_mr", 102, "stack/root", "stack/shared-mid"),
        pr("pr_t", 103, "stack/shared-mid", "stack/top"),
    ];
    let stacks = detect_stacks(&prs, "repo_stack", "acct_stack");
    assert_eq!(stacks.len(), 1);
    let stack = &stacks[0];
    assert_eq!(stack.kind, StackKind::Dag);
    let warning = stack
        .warning
        .as_ref()
        .expect("dag stacks must expose warning");
    assert!(
        warning.diamond_pr_ids.iter().any(|value| value == "pr_t"),
        "top node should be flagged as ambiguous"
    );
}
