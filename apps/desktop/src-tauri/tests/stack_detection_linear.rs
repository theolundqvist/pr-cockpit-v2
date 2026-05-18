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
        review_decision: Some("APPROVED".to_string()),
        check_rollup_state: Some("SUCCESS".to_string()),
        updated_at: 1,
    }
}

#[test]
fn detects_three_pr_linear_stack() {
    let prs = vec![
        pr("pr_c", 3, "main", "feature/c"),
        pr("pr_b", 2, "feature/c", "feature/b"),
        pr("pr_a", 1, "feature/b", "feature/a"),
    ];
    let stacks = detect_stacks(&prs, "repo_stack", "acct_stack");
    assert_eq!(stacks.len(), 1);
    let stack = &stacks[0];
    assert_eq!(stack.kind, StackKind::Linear);
    assert!(stack.warning.is_none());

    let positions = stack
        .nodes
        .iter()
        .map(|node| (node.pr_id.as_str(), node.position))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(positions.get("pr_c"), Some(&0));
    assert_eq!(positions.get("pr_b"), Some(&1));
    assert_eq!(positions.get("pr_a"), Some(&2));
}
