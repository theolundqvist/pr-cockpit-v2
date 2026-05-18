use std::collections::{HashMap, HashSet};

use ammonia::Builder;
use comrak::{markdown_to_html, Options};
use kuchiki::traits::*;
use kuchiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::storage::RenderCacheStore;
use crate::RENDERER_VERSION;

pub mod diff;

static TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"@[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?|#\d+|:[a-z0-9_+-]+:|\b[0-9a-f]{7,40}\b",
    )
    .expect("valid token regex")
});
static ALERT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*").expect("valid alert regex")
});
static SUGGESTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)<pre><code class="language-suggestion">(.+?)</code></pre>"#)
        .expect("valid suggestion regex")
});
static SUGGESTION_FENCE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?ms)^```suggestion[^\n]*\n(.*?)\n```").expect("valid suggestion fence regex")
});
static GH_ISSUE_OR_PR_LINK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^https://github\.com/([^/]+)/([^/]+)/(issues|pull)/(\d+)(#(?:issuecomment-\d+|discussion_r\d+))?/?$",
    )
        .expect("valid issue/pr autolink regex")
});
static GH_COMMIT_LINK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/commit/([0-9a-fA-F]{7,40})/?$")
        .expect("valid commit autolink regex")
});
static GH_LABEL_LINK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/labels/([^/?#]+)$")
        .expect("valid label autolink regex")
});
static EMOJI_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        (":rocket:", "🚀"),
        (":warning:", "⚠️"),
        (":white_check_mark:", "✅"),
        (":x:", "❌"),
        (":tada:", "🎉"),
        (":eyes:", "👀"),
        (":thumbsup:", "👍"),
    ])
});

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderCtx<'a> {
    pub repo: Option<&'a str>,
    pub cache: Option<&'a RenderCacheStore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionBlock {
    pub id: String,
    pub comment_id: String,
    pub body: String,
    pub start_line: i64,
    pub end_line: i64,
    pub side: String,
    pub original_commit_sha: String,
    pub suggestion_author_login: String,
}

#[derive(Clone, Debug)]
pub struct RenderedHtml {
    pub html: String,
    pub cache_hit: bool,
    pub content_hash: String,
    pub cache_key: String,
    pub renderer_version: &'static str,
}

pub fn render_comment(body: &str, ctx: &RenderCtx<'_>) -> RenderedHtml {
    let content_hash = sha256_hex(body.as_bytes());
    let cache_key = if let Some(cache) = ctx.cache {
        cache.cache_key(&content_hash, RENDERER_VERSION)
    } else {
        String::new()
    };

    if let Some(cache) = ctx.cache {
        if let Ok(Some(cached_html)) = cache.load_rendered_html(&cache_key) {
            return RenderedHtml {
                html: cached_html,
                cache_hit: true,
                content_hash,
                cache_key,
                renderer_version: RENDERER_VERSION,
            };
        }
    }

    let markdown_html = markdown_to_html(body, &comrak_options());
    let with_suggestions = postprocess_suggestion_blocks(&markdown_html);
    let with_alerts = postprocess_alerts(&with_suggestions);
    let with_links = postprocess_autolinks_and_emoji(&with_alerts, ctx.repo);
    let sanitized = sanitize_rendered_html(&with_links);

    if let Some(cache) = ctx.cache {
        let _ = cache.store_rendered_html(&cache_key, &sanitized);
    }

    RenderedHtml {
        html: sanitized,
        cache_hit: false,
        content_hash,
        cache_key,
        renderer_version: RENDERER_VERSION,
    }
}

fn comrak_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.math_dollars = true;
    options.render.r#unsafe = true;
    options
}

fn postprocess_suggestion_blocks(input: &str) -> String {
    SUGGESTION_RE
        .replace_all(input, |caps: &regex::Captures<'_>| {
            format!(
                r#"<div class="suggestion-block"><pre><code data-language="suggestion">{}</code></pre></div>"#,
                &caps[1]
            )
        })
        .to_string()
}

pub fn extract_suggestion_blocks(
    comment_id: &str,
    comment_body: &str,
    start_line: Option<i64>,
    line: Option<i64>,
    side: Option<&str>,
    original_commit_sha: Option<&str>,
    suggestion_author_login: Option<&str>,
) -> Vec<SuggestionBlock> {
    let start = start_line.or(line).unwrap_or(0);
    let end = line.or(start_line).unwrap_or(start);
    let resolved_side = side.unwrap_or("RIGHT").to_string();
    let resolved_author = suggestion_author_login.unwrap_or("ghost").to_string();
    let resolved_original_commit_sha = original_commit_sha.unwrap_or_default().to_string();

    SUGGESTION_FENCE_RE
        .captures_iter(comment_body)
        .enumerate()
        .map(|(index, captures)| SuggestionBlock {
            id: format!("{comment_id}:{index}"),
            comment_id: comment_id.to_string(),
            body: captures
                .get(1)
                .map(|capture| capture.as_str().to_string())
                .unwrap_or_default(),
            start_line: start,
            end_line: end,
            side: resolved_side.clone(),
            original_commit_sha: resolved_original_commit_sha.clone(),
            suggestion_author_login: resolved_author.clone(),
        })
        .collect()
}

fn postprocess_alerts(input: &str) -> String {
    let document = kuchiki::parse_html().one(input.to_string());
    let mut updates: Vec<(NodeRef, String, String, Vec<String>)> = Vec::new();

    if let Ok(selection) = document.select("blockquote") {
        for matched in selection {
            let blockquote = matched.as_node().clone();
            let paragraphs: Vec<NodeRef> = blockquote
                .children()
                .filter(|child| {
                    child
                        .as_element()
                        .map(|el| el.name.local.as_ref() == "p")
                        .unwrap_or(false)
                })
                .collect();
            let Some(first) = paragraphs.first() else {
                continue;
            };

            let text = first.text_contents();
            let trimmed = text.trim_start();
            let Some(captures) = ALERT_RE.captures(trimmed) else {
                continue;
            };

            let marker = captures.get(0).map(|m| m.as_str()).unwrap_or_default();
            let kind = captures
                .get(1)
                .map(|m| m.as_str().to_lowercase())
                .unwrap_or_else(|| "note".to_string());
            let title = capitalize(&kind);
            let remainder = trimmed
                .strip_prefix(marker)
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            let mut content_nodes: Vec<String> = Vec::new();
            if !remainder.is_empty() {
                content_nodes.push(format!("<p>{}</p>", escape_html(&remainder)));
            }
            for sibling in paragraphs.iter().skip(1) {
                content_nodes.push(sibling.to_string());
            }
            updates.push((blockquote, kind, title, content_nodes));
        }
    }

    for (blockquote, kind, title, content_nodes) in updates {
        if blockquote.parent().is_none() {
            continue;
        };
        if let Some(element) = blockquote.as_element() {
            element
                .attributes
                .borrow_mut()
                .insert("class", format!("markdown-alert markdown-alert-{kind}"));
        }
        if let Ok(children) = blockquote.select("p") {
            for p in children {
                p.as_node().detach();
            }
        }

        let replacement_html = if content_nodes.is_empty() {
            format!(r#"<p class="markdown-alert-title">{title}</p>"#)
        } else {
            format!(
                r#"<p class="markdown-alert-title">{title}</p>{}"#,
                content_nodes.join("")
            )
        };
        let fragment = kuchiki::parse_html().one(format!("<div>{replacement_html}</div>"));
        if let Ok(nodes) = fragment.select("div > p") {
            for node in nodes {
                blockquote.append(node.as_node().clone());
            }
        }
    }

    document.to_string()
}

fn postprocess_autolinks_and_emoji(input: &str, repo: Option<&str>) -> String {
    let document = kuchiki::parse_html().one(input.to_string());
    normalize_github_autolink_labels(&document, repo);
    let text_nodes: Vec<NodeRef> = document
        .descendants()
        .filter(|node| node.as_text().is_some())
        .collect();

    for text_node in text_nodes {
        if should_skip_text_replacement(&text_node) {
            continue;
        }

        let original = text_node
            .as_text()
            .map(|t| t.borrow().to_string())
            .unwrap_or_default();
        let replaced = rewrite_inline_tokens(&original, repo);
        if replaced == escape_html(&original) {
            continue;
        }

        let fragment = kuchiki::parse_html().one(format!("<span>{replaced}</span>"));
        if let Ok(span) = fragment.select_first("span") {
            let nodes: Vec<NodeRef> = span.as_node().children().collect();
            for node in nodes {
                text_node.insert_before(node);
            }
            text_node.detach();
        }
    }

    document.to_string()
}

fn normalize_github_autolink_labels(document: &NodeRef, repo: Option<&str>) {
    let Ok(selection) = document.select("a[href]") else {
        return;
    };
    let mut updates: Vec<(NodeRef, String)> = Vec::new();
    for anchor in selection {
        let href = {
            let attrs = anchor.attributes.borrow();
            attrs.get("href").map(str::to_owned)
        };
        let Some(href) = href else {
            continue;
        };
        let Some(label) = github_autolink_label(&href, repo) else {
            continue;
        };
        if !anchor_text_matches_href(anchor.as_node(), &href) {
            continue;
        }
        updates.push((anchor.as_node().clone(), label));
    }
    for (anchor_node, label) in updates {
        let children: Vec<NodeRef> = anchor_node.children().collect();
        for child in children {
            child.detach();
        }
        anchor_node.append(NodeRef::new_text(label));
    }
}

fn github_autolink_label(href: &str, repo: Option<&str>) -> Option<String> {
    if let Some(caps) = GH_ISSUE_OR_PR_LINK_RE.captures(href) {
        let owner = caps.get(1)?.as_str();
        let target_repo = caps.get(2)?.as_str();
        let number = caps.get(4)?.as_str();
        let target = format!("{owner}/{target_repo}");
        let same_repo = repo
            .map(|value| value.eq_ignore_ascii_case(&target))
            .unwrap_or(false);
        let mut label = if same_repo {
            format!("#{number}")
        } else {
            format!("{target}#{number}")
        };
        if caps.get(5).is_some() {
            label.push_str(" (comment)");
        }
        return Some(label);
    }
    if let Some(caps) = GH_COMMIT_LINK_RE.captures(href) {
        let owner = caps.get(1)?.as_str();
        let target_repo = caps.get(2)?.as_str();
        let sha = caps.get(3)?.as_str().to_lowercase();
        let target = format!("{owner}/{target_repo}");
        if repo
            .map(|value| value.eq_ignore_ascii_case(&target))
            .unwrap_or(false)
        {
            return Some(sha.chars().take(7).collect());
        }
        return Some(format!(
            "{target}@{}",
            sha.chars().take(7).collect::<String>()
        ));
    }
    if let Some(caps) = GH_LABEL_LINK_RE.captures(href) {
        let owner = caps.get(1)?.as_str();
        let target_repo = caps.get(2)?.as_str();
        let target = format!("{owner}/{target_repo}");
        if repo
            .map(|value| value.eq_ignore_ascii_case(&target))
            .unwrap_or(false)
        {
            return Some(caps.get(3)?.as_str().replace("%20", " "));
        }
    }
    None
}

fn anchor_text_matches_href(anchor: &NodeRef, href: &str) -> bool {
    let text = anchor.text_contents().trim().to_string();
    !text.is_empty() && normalize_link_value(&text) == normalize_link_value(href)
}

fn normalize_link_value(value: &str) -> String {
    value.trim_end_matches('/').to_ascii_lowercase()
}

fn sanitize_rendered_html(input: &str) -> String {
    let mut builder = Builder::default();
    builder
        .add_tags([
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
            "input",
            "blockquote",
            "div",
        ])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("code", ["class", "data-language"])
        .add_tag_attributes("a", ["href", "target"])
        .add_tag_attributes("blockquote", ["class"])
        .add_tag_attributes("div", ["class"])
        .link_rel(Some("noopener noreferrer"))
        .url_schemes(
            ["http", "https", "mailto"]
                .into_iter()
                .collect::<HashSet<_>>(),
        );
    builder.clean(input).to_string()
}

fn rewrite_inline_tokens(input: &str, repo: Option<&str>) -> String {
    let mut output = String::new();
    let mut last = 0;

    for m in TOKEN_RE.find_iter(input) {
        output.push_str(&escape_html(&input[last..m.start()]));
        let token = m.as_str();
        let prev = input[..m.start()].chars().next_back();
        let next = input[m.end()..].chars().next();

        if token.starts_with('@') && is_mention_boundary(prev, next) {
            let user = &token[1..];
            output.push_str(&format!(
                r#"<a href="https://github.com/{user}" rel="noopener noreferrer">@{user}</a>"#
            ));
        } else if token.starts_with('#') && repo.is_some() && is_boundary(prev) && is_boundary(next)
        {
            let issue_number = &token[1..];
            let repo = repo.expect("checked is_some");
            output.push_str(&format!(
                r#"<a href="https://github.com/{repo}/issues/{issue_number}" rel="noopener noreferrer">#{issue_number}</a>"#
            ));
        } else if token.starts_with(':') && token.ends_with(':') {
            if let Some(emoji) = EMOJI_MAP.get(token) {
                output.push_str(emoji);
            } else {
                output.push_str(&escape_html(token));
            }
        } else if repo.is_some()
            && token.chars().all(|c| c.is_ascii_hexdigit())
            && (7..=40).contains(&token.len())
            && is_boundary(prev)
            && is_boundary(next)
        {
            let repo = repo.expect("checked is_some");
            output.push_str(&format!(
                r#"<a href="https://github.com/{repo}/commit/{token}" rel="noopener noreferrer">{token}</a>"#
            ));
        } else {
            output.push_str(&escape_html(token));
        }

        last = m.end();
    }

    output.push_str(&escape_html(&input[last..]));
    output
}

fn should_skip_text_replacement(text_node: &NodeRef) -> bool {
    for ancestor in text_node.ancestors() {
        let Some(element) = ancestor.as_element() else {
            continue;
        };
        let name = element.name.local.to_string();
        if matches!(
            name.as_str(),
            "a" | "code" | "pre" | "script" | "style" | "textarea"
        ) {
            return true;
        }
    }
    false
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

fn is_mention_boundary(prev: Option<char>, next: Option<char>) -> bool {
    is_boundary(prev) && is_boundary(next)
}

fn is_boundary(value: Option<char>) -> bool {
    value
        .map(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(true)
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_scripts() {
        let ctx = RenderCtx::default();
        let rendered = render_comment("<script>alert('x')</script>hello", &ctx);
        assert!(!rendered.html.contains("<script"));
        assert!(rendered.html.contains("hello"));
    }

    #[test]
    fn normalizes_github_issue_and_comment_autolinks() {
        let rendered = render_comment(
            "xref: https://github.com/kubernetes/enhancements/pull/6015\n\nhttps://github.com/cli/cli/issues/13338#issuecomment-4439054677",
            &RenderCtx {
                repo: Some("kubernetes/kubernetes"),
                cache: None,
            },
        );
        assert!(rendered.html.contains(">kubernetes/enhancements#6015<"));
        assert!(rendered.html.contains(">cli/cli#13338 (comment)<"));
    }

    #[test]
    fn normalizes_same_repo_pr_autolinks() {
        let rendered = render_comment(
            "/close\nin favor of https://github.com/kubernetes/kubernetes/pull/138914",
            &RenderCtx {
                repo: Some("kubernetes/kubernetes"),
                cache: None,
            },
        );
        assert!(rendered.html.contains(">#138914<"));
    }

    #[test]
    fn normalizes_same_repo_label_links() {
        let rendered = render_comment(
            "tagged as https://github.com/cli/cli/labels/help%20wanted",
            &RenderCtx {
                repo: Some("cli/cli"),
                cache: None,
            },
        );
        assert!(rendered.html.contains(">help wanted<"));
    }

    #[test]
    fn extracts_suggestion_blocks_from_markdown_fences() {
        let body = "before\n```suggestion\nlet x = 1;\n```\n\n```suggestion\nlet y = 2;\n```\n";
        let blocks = extract_suggestion_blocks(
            "comment-1",
            body,
            Some(10),
            Some(11),
            Some("RIGHT"),
            Some("abc123"),
            Some("octocat"),
        );
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].id, "comment-1:0");
        assert_eq!(blocks[0].body, "let x = 1;");
        assert_eq!(blocks[1].id, "comment-1:1");
        assert_eq!(blocks[1].body, "let y = 2;");
        assert_eq!(blocks[0].start_line, 10);
        assert_eq!(blocks[0].end_line, 11);
        assert_eq!(blocks[0].side, "RIGHT");
        assert_eq!(blocks[0].original_commit_sha, "abc123");
        assert_eq!(blocks[0].suggestion_author_login, "octocat");
    }
}
