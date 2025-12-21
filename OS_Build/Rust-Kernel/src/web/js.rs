#![allow(dead_code)]
extern crate alloc;

use alloc::{string::{String, ToString}, vec::Vec};

use super::dom::{Document, NodeKind};

/// Extremely small JS "runner" just to make basic pages that do `document.write("...")` work.
///
/// Supported:
/// - document.write("text")
/// - document.writeln("text")
///
/// It does not implement variables, loops, DOM APIs, etc.
/// It is intended as a stepping stone while you wire a real JS engine later.
pub fn run_scripts(doc: &mut Document, scripts: &[String]) {
    for s in scripts {
        for w in extract_document_writes(s) {
            append_text_to_document(doc, &w);
        }
    }
}

fn extract_document_writes(js: &str) -> Vec<String> {
    let mut out = Vec::new();
    let hay = js.as_bytes();
    let mut i = 0usize;

    while i + 14 < hay.len() {
        // search for "document.write" or "document.writeln"
        if hay[i..].starts_with(b"document.write") || hay[i..].starts_with(b"document.writeln") {
            // advance to '('
            while i < hay.len() && hay[i] != b'(' { i += 1; }
            if i >= hay.len() { break; }
            i += 1;
            // skip ws
            while i < hay.len() && matches!(hay[i], b' ' | b'\n' | b'\r' | b'\t') { i += 1; }
            if i >= hay.len() { break; }

            // parse string literal "..." or '...'
            let q = hay[i];
            if q != b'"' && q != b'\'' { continue; }
            i += 1;
            let start = i;
            while i < hay.len() && hay[i] != q {
                // skip escaped quotes
                if hay[i] == b'\\' && i + 1 < hay.len() { i += 2; continue; }
                i += 1;
            }
            if i >= hay.len() { break; }
            let raw = String::from_utf8_lossy(&hay[start..i]).to_string();
            let txt = unescape_basic(&raw);
            out.push(txt);
            // skip quote and continue
            i += 1;
        } else {
            i += 1;
        }
    }

    out
}

fn unescape_basic(s: &str) -> String {
    // Minimal string escapes: \n \r \t \" \' \\
    let mut out = String::new();
    let mut it = s.bytes();
    while let Some(b) = it.next() {
        if b == b'\\' {
            if let Some(n) = it.next() {
                match n {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'\\' => out.push('\\'),
                    b'"' => out.push('"'),
                    b'\'' => out.push('\''),
                    _ => {
                        out.push('\\');
                        out.push(n as char);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(b as char);
        }
    }
    out
}

fn append_text_to_document(doc: &mut Document, text: &str) {
    // Append to root for now. A better approach is to find <body>.
    let parent = doc.root;
    doc.push(parent, NodeKind::Text(text.to_string()));
}
