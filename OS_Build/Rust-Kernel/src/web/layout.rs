#![allow(dead_code)]
extern crate alloc;

use alloc::{string::{String, ToString}, vec::Vec};

use super::{css::{self, Display, Style, Rule}, dom::{Document, NodeKind}};

#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    rules: &'a [Rule],
    width: usize,
}

fn default_display(tag: &str) -> Display {
    match tag {
        "div"|"p"|"section"|"article"|"header"|"footer"|"nav"|"main"|"ul"|"ol"|"li"|"pre"|"blockquote"|
        "h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"table"|"tr"|"td" => Display::Block,
        "br" => Display::Block,
        _ => Display::Inline,
    }
}

fn collapse_ws(s: &str) -> Vec<&str> {
    s.split_whitespace().filter(|w| !w.is_empty()).collect()
}

fn push_wrapped(out: &mut Vec<String>, cur: &mut String, words: &[&str], width: usize) {
    for w in words {
        if cur.is_empty() {
            cur.push_str(w);
        } else if cur.len() + 1 + w.len() > width {
            out.push(cur.trim_end().to_string());
            cur.clear();
            cur.push_str(w);
        } else {
            cur.push(' ');
            cur.push_str(w);
        }
    }
}

fn flush(out: &mut Vec<String>, cur: &mut String) {
    if !cur.trim().is_empty() {
        out.push(cur.trim_end().to_string());
    }
    cur.clear();
}

fn is_hidden(st: &Style, tag: &str) -> bool {
    matches!(st.display, Some(Display::None)) || tag == "script" || tag == "style"
}

pub fn render_text_lines(doc: &Document, rules: &[Rule], width_chars: usize) -> Vec<String> {
    let ctx = RenderCtx { rules, width: width_chars.max(20) };
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();

    walk(doc, doc.root, &ctx, &mut out, &mut cur);

    flush(&mut out, &mut cur);
    if out.is_empty() { out.push("(empty)".into()); }
    out
}

fn walk(doc: &Document, idx: usize, ctx: &RenderCtx<'_>, out: &mut Vec<String>, cur: &mut String) {
    match &doc.nodes[idx].kind {
        NodeKind::Document => {
            for &ch in &doc.nodes[idx].children {
                walk(doc, ch, ctx, out, cur);
            }
        }
        NodeKind::Text(t) => {
            // Keep preformatted if parent is <pre>
            let is_pre = doc.nodes[idx].parent.and_then(|p| doc.element_tag(p)).map(|t| t == "pre").unwrap_or(false);
            if is_pre {
                // split by \n and push as lines, no wrapping
                for line in t.replace('\r', "").split('\n') {
                    flush(out, cur);
                    if !line.is_empty() { out.push(line.to_string()); }
                }
            } else {
                let words = collapse_ws(t);
                push_wrapped(out, cur, &words, ctx.width);
            }
        }
        NodeKind::Element(el) => {
            let tag = el.tag.as_str();
            let id = doc.element_id(idx);
            let classes = doc.element_classes(idx);
            let inline = el.attrs.iter().find(|(k,_)| k == "style").map(|(_,v)| css::parse_inline_style(v)).unwrap_or_default();
            let mut st = css::apply_rules(tag, id, &classes, ctx.rules, &inline);

            // Set default display if not specified
            if st.display.is_none() {
                st.display = Some(default_display(tag));
            }

            if is_hidden(&st, tag) {
                return;
            }

            // block open behaviors
            if matches!(st.display, Some(Display::Block)) || tag == "br" {
                flush(out, cur);
            }

            // simple headings
            if matches!(tag, "h1"|"h2"|"h3") {
                flush(out, cur);
            }

            // lists: add bullet for <li>
            if tag == "li" {
                flush(out, cur);
                cur.push_str("• ");
            }

            // anchors: if it has href, we render as "text (href)".
            let href = el.attrs.iter().find(|(k,_)| k == "href").map(|(_,v)| v.as_str());

            let mut before_len = out.len();
            let cur_before = cur.clone();

            for &ch in &doc.nodes[idx].children {
                walk(doc, ch, ctx, out, cur);
            }

            if tag == "a" {
                // Append href if present and we rendered some text.
                if let Some(h) = href {
                    // if current line has text, append; else add new line
                    if !cur.is_empty() {
                        if cur.len() + 3 + h.len() <= ctx.width {
                            cur.push_str(" (");
                            cur.push_str(h);
                            cur.push(')');
                        } else {
                            flush(out, cur);
                            out.push(format!("({})", h));
                        }
                    } else if out.len() > before_len {
                        // last line is in out
                        let last = out.last_mut().unwrap();
                        last.push_str(" (");
                        last.push_str(h);
                        last.push(')');
                    }
                }
            }

            // block close behaviors
            if matches!(st.display, Some(Display::Block)) || matches!(tag, "p"|"div"|"li"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6") {
                flush(out, cur);
            }
        }
    }
}
