#![allow(dead_code)]
extern crate alloc;

use alloc::{string::{String, ToString}, vec::Vec};

use super::dom::{Document, NodeKind, ElementData};

fn is_ws(b: u8) -> bool { matches!(b, b' ' | b'\n' | b'\r' | b'\t') }

fn lower_ascii(s: &str) -> String { s.to_ascii_lowercase() }

fn decode_entities(mut s: String) -> String {
    // Minimal entity decoding. Expand as you need.
    s = s.replace("&amp;", "&");
    s = s.replace("&lt;", "<");
    s = s.replace("&gt;", ">");
    s = s.replace("&quot;", "\"");
    s = s.replace("&nbsp;", " ");
    s
}

fn read_ident(input: &[u8], mut i: usize) -> (String, usize) {
    let start = i;
    while i < input.len() {
        let b = input[i];
        if is_ws(b) || b == b'>' || b == b'/' || b == b'=' { break; }
        i += 1;
    }
    (String::from_utf8_lossy(&input[start..i]).to_string(), i)
}

fn read_attr_value(input: &[u8], mut i: usize) -> (String, usize) {
    while i < input.len() && is_ws(input[i]) { i += 1; }
    if i >= input.len() { return (String::new(), i); }

    let b = input[i];
    if b == b'"' || b == b'\'' {
        let q = b;
        i += 1;
        let start = i;
        while i < input.len() && input[i] != q { i += 1; }
        let v = String::from_utf8_lossy(&input[start..i]).to_string();
        if i < input.len() { i += 1; }
        (v, i)
    } else {
        let start = i;
        while i < input.len() && !is_ws(input[i]) && input[i] != b'>' { i += 1; }
        (String::from_utf8_lossy(&input[start..i]).to_string(), i)
    }
}

fn is_void_tag(tag: &str) -> bool {
    matches!(tag, "br"|"img"|"meta"|"link"|"hr"|"input"|"area"|"base"|"col"|"embed"|"param"|"source"|"track"|"wbr")
}

fn is_rawtext_tag(tag: &str) -> bool {
    matches!(tag, "script" | "style")
}

fn find_end_tag(input: &[u8], mut i: usize, tag: &str) -> usize {
    // Find `</tag` case-insensitive. Returns index of '<' or input.len().
    let needle = alloc::format!("</{}", tag);
    while i + needle.len() <= input.len() {
        if input[i] == b'<' {
            // compare case-insensitive
            let end = (i + needle.len()).min(input.len());
            let s = String::from_utf8_lossy(&input[i..end]).to_ascii_lowercase();
            if s == needle {
                return i;
            }
        }
        i += 1;
    }
    input.len()
}

pub struct ParsedPage {
    pub doc: Document,
    pub style_texts: Vec<String>,
    pub script_texts: Vec<String>,
}

pub fn parse(input: &[u8]) -> ParsedPage {
    let mut doc = Document::new();
    let mut stack: Vec<usize> = vec![doc.root];

    let mut style_texts: Vec<String> = Vec::new();
    let mut script_texts: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < input.len() {
        if input[i] != b'<' {
            // text node
            let start = i;
            while i < input.len() && input[i] != b'<' { i += 1; }
            let raw = String::from_utf8_lossy(&input[start..i]).to_string();
            let t = decode_entities(raw);
            let t = t.replace('\r', "");
            // Keep whitespace, but collapse huge runs later in layout.
            if !t.is_empty() {
                let parent = *stack.last().unwrap();
                doc.push(parent, NodeKind::Text(t));
            }
            continue;
        }

        // Tag
        i += 1;
        if i >= input.len() { break; }

        // comment
        if i + 2 < input.len() && input[i] == b'!' && input[i+1] == b'-' && input[i+2] == b'-' {
            // skip <!-- ... -->
            i += 3;
            while i + 2 < input.len() {
                if input[i] == b'-' && input[i+1] == b'-' && input[i+2] == b'>' {
                    i += 3;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // end tag
        if input[i] == b'/' {
            i += 1;
            let (name, ni) = read_ident(input, i);
            let tag = lower_ascii(&name);
            i = ni;
            while i < input.len() && input[i] != b'>' { i += 1; }
            if i < input.len() { i += 1; }

            // pop until matching tag (best-effort)
            if stack.len() > 1 {
                while stack.len() > 1 {
                    let top = *stack.last().unwrap();
                    if doc.element_tag(top) == Some(tag.as_str()) {
                        stack.pop();
                        break;
                    }
                    stack.pop();
                }
            }
            continue;
        }

        // start tag
        let (name, ni) = read_ident(input, i);
        let tag = lower_ascii(&name);
        i = ni;

        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut self_close = false;

        while i < input.len() {
            while i < input.len() && is_ws(input[i]) { i += 1; }
            if i >= input.len() { break; }
            if input[i] == b'>' { i += 1; break; }
            if input[i] == b'/' {
                // />
                self_close = true;
                i += 1;
                while i < input.len() && input[i] != b'>' { i += 1; }
                if i < input.len() { i += 1; }
                break;
            }

            let (k, ki) = read_ident(input, i);
            i = ki;
            let key = lower_ascii(&k);
            while i < input.len() && is_ws(input[i]) { i += 1; }
            let mut val = String::new();
            if i < input.len() && input[i] == b'=' {
                i += 1;
                let (v, vi) = read_attr_value(input, i);
                val = v;
                i = vi;
            }
            if !key.is_empty() { attrs.push((key, val)); }
        }

        let parent = *stack.last().unwrap();
        let el = doc.push(parent, NodeKind::Element(ElementData { tag: tag.clone(), attrs }));

        if is_rawtext_tag(&tag) && !self_close {
            // Collect raw text until </tag>
            let end = find_end_tag(input, i, &tag);
            let raw = String::from_utf8_lossy(&input[i..end]).to_string();
            if tag == "style" { style_texts.push(raw.clone()); }
            if tag == "script" { script_texts.push(raw.clone()); }

            // Store as text child too (not rendered by layout later, but useful)
            if !raw.is_empty() {
                doc.push(el, NodeKind::Text(raw));
            }

            i = end;
            // consume end tag
            while i < input.len() && input[i] != b'>' { i += 1; }
            if i < input.len() { i += 1; }
            continue;
        }

        if self_close || is_void_tag(&tag) {
            // no push
        } else {
            stack.push(el);
        }
    }

    ParsedPage { doc, style_texts, script_texts }
}
