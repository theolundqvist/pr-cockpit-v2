use tempfile::tempdir;

use pr_cockpit_desktop::render::{render_comment, RenderCtx};
use pr_cockpit_desktop::storage::RenderCacheStore;
use pr_cockpit_desktop::RENDERER_VERSION;

#[test]
fn renders_alert_block() {
    let body = "> [!WARNING]\n> Heads up";
    let rendered = render_comment(body, &RenderCtx::default());
    assert!(rendered.html.contains("markdown-alert-warning"));
    assert!(rendered.html.contains("<p>Warning</p>"));
}

#[test]
fn renders_suggestion_fence() {
    let body = "```suggestion\nlet x = 1;\n```";
    let rendered = render_comment(body, &RenderCtx::default());
    assert!(rendered.html.contains("suggestion-block"));
    assert!(rendered.html.contains("data-language=\"suggestion\""));
}

#[test]
fn renders_user_autolink() {
    let body = "cc @octocat";
    let rendered = render_comment(body, &RenderCtx::default());
    assert!(rendered
        .html
        .contains("href=\"https://github.com/octocat\""));
}

#[test]
fn renders_issue_autolink() {
    let body = "Fixes #123";
    let ctx = RenderCtx {
        repo: Some("cli/cli"),
        cache: None,
    };
    let rendered = render_comment(body, &ctx);
    assert!(rendered
        .html
        .contains("href=\"https://github.com/cli/cli/issues/123\""));
}

#[test]
fn renders_sha_autolink() {
    let body = "regressed in deadbee";
    let ctx = RenderCtx {
        repo: Some("cli/cli"),
        cache: None,
    };
    let rendered = render_comment(body, &ctx);
    assert!(rendered
        .html
        .contains("href=\"https://github.com/cli/cli/commit/deadbee\""));
}

#[test]
fn renders_emoji_shortcode() {
    let body = "Ship it :rocket:";
    let rendered = render_comment(body, &RenderCtx::default());
    assert!(rendered.html.contains("🚀"));
    assert!(!rendered.html.contains(":rocket:"));
}

#[test]
fn uses_render_cache_on_second_render() {
    let dir = tempdir().expect("tmpdir");
    let cache = RenderCacheStore::memory(dir.path()).expect("cache");
    let ctx = RenderCtx {
        repo: Some("cli/cli"),
        cache: Some(&cache),
    };
    let body = "Hello @octocat #7 deadbee :rocket:";
    let first = render_comment(body, &ctx);
    let second = render_comment(body, &ctx);
    let expected_key = cache.cache_key(&first.content_hash, RENDERER_VERSION);

    assert!(!first.cache_hit);
    assert!(second.cache_hit);
    assert_eq!(first.cache_key, expected_key);
    assert_eq!(second.cache_key, expected_key);
    assert_eq!(
        cache.rendered_ref_count(&expected_key).expect("ref count"),
        1
    );
}

#[test]
fn mini_corpus_smoke_snapshot() {
    let fixtures = [
        ("1", "Hello **world**", None, "<strong>world</strong>"),
        ("2", "|a|b|\n|-|-|\n|1|2|", None, "<table>"),
        ("3", "- [x] done", None, "type=\"checkbox\""),
        (
            "4",
            "> [!NOTE]\n> remember this",
            None,
            "markdown-alert-note",
        ),
        (
            "5",
            "```suggestion\nreturn Ok(());\n```",
            None,
            "suggestion-block",
        ),
        ("6", "cc @rustlang", None, "https://github.com/rustlang"),
        (
            "7",
            "duplicates #42",
            Some("rust-lang/rust"),
            "https://github.com/rust-lang/rust/issues/42",
        ),
        (
            "8",
            "see deadbeef",
            Some("rust-lang/rust"),
            "https://github.com/rust-lang/rust/commit/deadbeef",
        ),
        ("9", "launch :rocket:", None, "🚀"),
        ("10", "<script>bad()</script>ok", None, "ok"),
    ];

    for (id, body, repo, expected) in fixtures {
        let ctx = RenderCtx { repo, cache: None };
        let rendered = render_comment(body, &ctx);
        assert!(
            rendered.html.contains(expected),
            "fixture {id} missing {expected}"
        );
        assert!(
            !rendered.html.contains("<script"),
            "fixture {id} leaked script"
        );
    }
}
