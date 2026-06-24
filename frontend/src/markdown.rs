//! Tiny, XSS-safe inline-markdown renderer for chat bubbles.
//!
//! Roleplay chat leans on a small subset of markdown — `*italics*` for actions,
//! `**bold**`, `` `code` ``, `> quotes`, and `![](image)` — so we parse exactly
//! that into a node tree and render real DOM elements through Leptos `view!`
//! (NOT `innerHTML`): text/markup is escaped by the framework, and image `src`
//! URLs are additionally restricted to an http(s)/`data:image` scheme allowlist
//! (`is_safe_image_url`), so model output can't inject markup or odd-scheme URLs.
//!
//! Delimiters (`* ` `` ` `` `!` `[` `]` `(` `)` `>`) are all ASCII, so every
//! byte index we slice at is a UTF-8 boundary even when the content is not.

use leptos::prelude::*;

/// An inline node within a line of text.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Em(Vec<Inline>),
    Strong(Vec<Inline>),
    Code(String),
    Image(String),
}

/// Find `delim` at or after byte `from`, returning its start index.
fn find_at(s: &str, from: usize, delim: &str) -> Option<usize> {
    if from > s.len() {
        return None;
    }
    s[from..].find(delim).map(|p| from + p)
}

/// Parse a single line/run of text into inline nodes. Unclosed delimiters are
/// emitted as literal text, so stray `*` or backticks never eat the rest.
pub fn parse_inline(s: &str) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let bytes = s.as_bytes();
    let n = s.len();
    let mut i = 0usize;
    let mut text_start = 0usize;

    while i < n {
        let b = bytes[i];
        let matched: Option<(Inline, usize)> = match b {
            b'`' => find_at(s, i + 1, "`")
                .map(|j| (Inline::Code(s[i + 1..j].to_string()), j + 1)),
            b'!' if s[i..].starts_with("![") => {
                find_at(s, i + 2, "](").and_then(|rb| {
                    // Balanced-paren scan: match the ')' that closes the '(' from
                    // "](", so URLs containing parens (e.g. `File_(1).png`) aren't
                    // truncated at the first ')'. All scanned bytes are ASCII, so
                    // the byte indices stay on UTF-8 boundaries.
                    let start = rb + 2;
                    let mut depth = 1i32;
                    let mut idx = start;
                    while idx < n {
                        match bytes[idx] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        idx += 1;
                    }
                    if depth != 0 {
                        return None; // unclosed → emit literally
                    }
                    let url = s[start..idx].trim();
                    // Only accept safe schemes; otherwise fall through to literal
                    // text so a `javascript:`/odd-scheme URL never reaches an attr.
                    if is_safe_image_url(url) {
                        Some((Inline::Image(url.to_string()), idx + 1))
                    } else {
                        None
                    }
                })
            }
            b'*' => {
                if s[i..].starts_with("***") {
                    find_at(s, i + 3, "***").map(|j| {
                        (Inline::Strong(vec![Inline::Em(parse_inline(&s[i + 3..j]))]), j + 3)
                    })
                } else if s[i..].starts_with("**") {
                    find_at(s, i + 2, "**")
                        .map(|j| (Inline::Strong(parse_inline(&s[i + 2..j])), j + 2))
                } else {
                    find_at(s, i + 1, "*")
                        .map(|j| (Inline::Em(parse_inline(&s[i + 1..j])), j + 1))
                }
            }
            _ => None,
        };

        match matched {
            // Guard against zero/over-wide consumes and empty emphasis runs.
            Some((node, end)) if end > i && !is_empty_node(&node) => {
                if i > text_start {
                    out.push(Inline::Text(s[text_start..i].to_string()));
                }
                out.push(node);
                i = end;
                text_start = end;
            }
            _ => {
                // Advance one whole UTF-8 char.
                let step = s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                i += step;
            }
        }
    }
    if text_start < n {
        out.push(Inline::Text(s[text_start..n].to_string()));
    }
    out
}

/// True for emphasis/strong nodes that turned out to wrap nothing — checked
/// recursively so `***` ` *** `(Strong→Em→empty) and other nested-empty runs
/// don't render an empty `<strong><em></em></strong>`.
fn is_empty_node(node: &Inline) -> bool {
    match node {
        Inline::Em(v) | Inline::Strong(v) => v.iter().all(is_empty_node),
        Inline::Code(c) => c.is_empty(),
        Inline::Image(u) => u.is_empty(),
        Inline::Text(t) => t.is_empty(),
    }
}

/// Allow only image URL schemes that are safe to drop into an `<img src>`:
/// http(s), `data:image/...`, and scheme-less relative paths. Everything else
/// (e.g. `javascript:`, `data:text/html`, protocol-relative `//`) is rejected.
fn is_safe_image_url(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("http://")
        || l.starts_with("https://")
        || l.starts_with("data:image/")
        || (!l.contains(':') && !l.starts_with("//"))
}

/// Render a list of inline nodes to a Leptos view.
fn render_inline(nodes: Vec<Inline>) -> AnyView {
    nodes
        .into_iter()
        .map(|node| match node {
            Inline::Text(t) => t.into_any(),
            Inline::Em(inner) => view! { <em>{render_inline(inner)}</em> }.into_any(),
            Inline::Strong(inner) => view! { <strong>{render_inline(inner)}</strong> }.into_any(),
            Inline::Code(c) => view! { <code class="md-code">{c}</code> }.into_any(),
            Inline::Image(url) => {
                view! { <img class="msg__img" src=url alt="" loading="lazy" /> }.into_any()
            }
        })
        .collect_view()
        .into_any()
}

/// Render a full message: blockquotes (`> `) become `<blockquote>`; everything
/// else is grouped into paragraphs whose newlines are preserved via CSS
/// `white-space: pre-wrap` on `.md-para`.
pub fn render_message(text: &str) -> AnyView {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut blocks: Vec<AnyView> = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = trimmed.strip_prefix('>') {
            let mut quote = vec![rest.strip_prefix(' ').unwrap_or(rest).to_string()];
            i += 1;
            while i < lines.len() {
                let t = lines[i].trim_start();
                if let Some(r) = t.strip_prefix('>') {
                    quote.push(r.strip_prefix(' ').unwrap_or(r).to_string());
                    i += 1;
                } else {
                    break;
                }
            }
            let inner = quote.join("\n");
            blocks.push(
                view! { <blockquote class="md-quote">{render_inline(parse_inline(&inner))}</blockquote> }
                    .into_any(),
            );
        } else {
            let mut para = vec![lines[i].to_string()];
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with('>') {
                para.push(lines[i].to_string());
                i += 1;
            }
            let joined = para.join("\n");
            blocks.push(
                view! { <span class="md-para">{render_inline(parse_inline(&joined))}</span> }
                    .into_any(),
            );
        }
    }
    blocks.into_iter().collect_view().into_any()
}
