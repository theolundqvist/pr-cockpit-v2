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
}
