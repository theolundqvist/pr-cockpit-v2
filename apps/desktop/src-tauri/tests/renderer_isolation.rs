use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn renderer_does_not_reach_sqlite_or_github_directly() {
    let renderer_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
    let mut files = Vec::new();
    collect_renderer_files(&renderer_root, &mut files);

    let checks = [
        ("sqlite", true),
        ("better-sqlite3", true),
        ("octokit", true),
        ("graphql-request", true),
        ("fetch(\"https://api.github.com", false),
        ("fetch(`https://api.github.com", false),
    ];

    let mut violations = Vec::new();
    for file in files {
        let contents = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        for (line_index, line) in contents.lines().enumerate() {
            let normalized = line.to_ascii_lowercase();
            let is_type_only_import = normalized.trim_start().starts_with("import type");
            for (needle, allow_type_only_import) in &checks {
                if normalized.contains(needle) && !(*allow_type_only_import && is_type_only_import)
                {
                    violations.push(format!(
                        "{}:{} contains forbidden pattern `{needle}`",
                        file.display(),
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "renderer isolation violations found:\n{}",
        violations.join("\n")
    );
}

fn collect_renderer_files(root: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", root.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read directory entry: {error}"));
        let path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("failed to read file type for {}: {error}", path.display())
        });
        if file_type.is_dir() {
            collect_renderer_files(&path, files);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if matches!(extension, "ts" | "svelte") {
                files.push(path);
            }
        }
    }
}
