use anyhow::Result;
use desktop_lib::db::Db;
use desktop_lib::stacks::ops::{
    get_stack_op, rebase_stack, set_bool_setting, CommandGitOps, StackOperationStatus,
};
use tempfile::TempDir;

#[path = "stack_test_support.rs"]
mod stack_test_support;

#[tokio::test]
async fn stack_rebase_happy_rebases_all_branches() -> Result<()> {
    let git_fixture = stack_test_support::init_git_fixture()?;
    stack_test_support::seed_linear_git_stack(&git_fixture.repo_path)?;

    let db_dir = TempDir::new()?;
    let db = Db::open(db_dir.path()).await?;
    let repo_id = "repo-stack";
    let account_id = stack_test_support::seed_account_repo(
        &db,
        "stack-rebase-happy",
        repo_id,
        "octo",
        "hello-world",
    )
    .await?;

    let prs = vec![
        stack_test_support::pr_row(&account_id, repo_id, "pr_c", 3, "main", "feature/c"),
        stack_test_support::pr_row(&account_id, repo_id, "pr_b", 2, "feature/c", "feature/b"),
        stack_test_support::pr_row(&account_id, repo_id, "pr_a", 1, "feature/b", "feature/a"),
    ];
    stack_test_support::seed_prs(&db, &prs).await?;
    let stack_id = stack_test_support::seed_stack(&db, &account_id, repo_id, &prs).await?;
    stack_test_support::seed_worktree(
        &db,
        &account_id,
        repo_id,
        &git_fixture.repo_path,
        "main",
        Some("pr_a"),
    )
    .await?;
    set_bool_setting(&db, "force_push_with_lease", true).await?;

    let git = CommandGitOps;
    let op_id = rebase_stack(&db, &git, &account_id, &stack_id).await?;
    let operation = get_stack_op(&db, &op_id)
        .await?
        .expect("stack operation to exist");
    assert_eq!(operation.status, StackOperationStatus::Succeeded);
    assert_eq!(operation.current_step, Some(3));

    let merge_base_ab = stack_test_support::run_git(
        &git_fixture.repo_path,
        &["merge-base", "feature/a", "feature/b"],
    )?;
    let head_b = stack_test_support::run_git(&git_fixture.repo_path, &["rev-parse", "feature/b"])?;
    assert_eq!(merge_base_ab, head_b);

    let merge_base_bc = stack_test_support::run_git(
        &git_fixture.repo_path,
        &["merge-base", "feature/b", "feature/c"],
    )?;
    let head_c = stack_test_support::run_git(&git_fixture.repo_path, &["rev-parse", "feature/c"])?;
    assert_eq!(merge_base_bc, head_c);
    Ok(())
}
