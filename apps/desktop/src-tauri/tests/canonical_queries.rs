use std::path::{Path, PathBuf};

use anyhow::Result;

#[test]
fn graphql_queries_are_canonicalized() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("failed to locate workspace root"))?
        .to_path_buf();
    let desktop_root = workspace_root.join("apps/desktop");
    let canonical_dir = desktop_root.join("src-tauri/src/api/queries");

    let mut graphql_files = Vec::new();
    collect_files_with_ext(&desktop_root, "graphql", &mut graphql_files)?;
    for file in &graphql_files {
        let normalized = file.to_string_lossy().replace('\\', "/");
        assert!(
            normalized.contains("/src-tauri/src/api/queries/"),
            "stray graphql file outside canonical directory: {}",
            file.display()
        );
    }
    assert_eq!(
        graphql_files.len(),
        2,
        "expected exactly two canonical graphql files in {}",
        canonical_dir.display()
    );

    let mut rust_files = Vec::new();
    collect_files_with_ext(&desktop_root, "rs", &mut rust_files)?;
    let macro_marker = ["g", "ql!"].concat();
    for file in rust_files {
        if file.ends_with("tests/canonical_queries.rs") {
            continue;
        }
        let content = std::fs::read_to_string(&file)?;
        assert!(
            !content.contains(&macro_marker),
            "gql! macro usage is forbidden outside canonical queries: {}",
            file.display()
        );
    }

    Ok(())
}

fn collect_files_with_ext(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_ext(&path, extension, out)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(extension))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}
