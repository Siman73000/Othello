#![allow(dead_code)]
extern crate alloc;

use alloc::{string::{String, ToString}, vec::Vec};

#[derive(Clone, Debug)]
pub enum Display {
    Inline,
    Block,
    None,
}

#[derive(Clone, Debug, Default)]
pub struct Style {
    pub display: Option<Display>,
    pub color: Option<u32>, // 0xRRGGBB
    pub font_weight_bold: Option<bool>,
}

#[derive(Clone, Debug)]
pub enum Selector {
    Tag(String),
    Class(String),
    Id(String),
    TagClass(String, String),
    TagId(String, String),
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub selector: Selector,
    pub decls: Style,
    pub specificity: (u8, u8, u8), // (id, class, tag)
    pub order: u32,
}

fn is_ws(c: u8) -> bool { matches!(c, b' ' | b'\n' | b'\r' | b'\t') }

fn parse_hex_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if !s.starts_with('#') { return None; }
    let h = &s[1..];
    if h.len() == 6 {
        let v = u32::from_str_radix(h, 16).ok()?;
        return Some(v);
    }
    None
}

fn parse_display(v: &str) -> Option<Display> {
    let v = v.trim().to_ascii_lowercase();
    match v.as_str() {
        "block" => Some(Display::Block),
        "inline" => Some(Display::Inline),
        "none" => Some(Display::None),
        _ => None,
    }
}

fn parse_font_weight(v: &str) -> Option<bool> {
    let v = v.trim().to_ascii_lowercase();
    match v.as_str() {
        "bold" | "700" | "800" | "900" => Some(true),
        "normal" | "400" | "500" => Some(false),
        _ => None,
    }
}

fn parse_selector(s: &str) -> Option<(Selector, (u8,u8,u8))> {
    let s = s.trim();
    if s.is_empty() { return None; }
    if s.starts_with('#') {
        return Some((Selector::Id(s[1..].to_ascii_lowercase()), (1,0,0)));
    }
    if s.starts_with('.') {
        return Some((Selector::Class(s[1..].to_ascii_lowercase()), (0,1,0)));
    }
    // tag.class or tag#id
    if let Some(p) = s.find('.') {
        let t = s[..p].to_ascii_lowercase();
        let c = s[p+1..].to_ascii_lowercase();
        if !t.is_empty() && !c.is_empty() {
            return Some((Selector::TagClass(t, c), (0,1,1)));
        }
    }
    if let Some(p) = s.find('#') {
        let t = s[..p].to_ascii_lowercase();
        let id = s[p+1..].to_ascii_lowercase();
        if !t.is_empty() && !id.is_empty() {
            return Some((Selector::TagId(t, id), (1,0,1)));
        }
    }
    Some((Selector::Tag(s.to_ascii_lowercase()), (0,0,1)))
}

fn parse_decls(s: &str) -> Style {
    let mut st = Style::default();
    for part in s.split(';') {
        let mut it = part.splitn(2, ':');
        let k = it.next().unwrap_or("").trim().to_ascii_lowercase();
        let v = it.next().unwrap_or("").trim();
        if k.is_empty() { continue; }
        match k.as_str() {
            "display" => st.display = parse_display(v),
            "color" => st.color = parse_hex_color(v),
            "font-weight" => st.font_weight_bold = parse_font_weight(v),
            _ => {}
        }
    }
    st
}

pub fn parse_stylesheet(css: &str) -> Vec<Rule> {
    // Extremely small CSS parser: supports "selector { decls }"
    let bytes = css.as_bytes();
    let mut i = 0usize;
    let mut rules: Vec<Rule> = Vec::new();
    let mut order: u32 = 0;

    while i < bytes.len() {
        while i < bytes.len() && is_ws(bytes[i]) { i += 1; }
        if i >= bytes.len() { break; }

        // skip comments /* ... */
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i+1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i+1] == b'/' { i += 2; break; }
                i += 1;
            }
            continue;
        }

        // read selector until '{'
        let sel_start = i;
        while i < bytes.len() && bytes[i] != b'{' { i += 1; }
        if i >= bytes.len() { break; }
        let sel = String::from_utf8_lossy(&bytes[sel_start..i]).to_string();
        i += 1; // skip '{'

        let decl_start = i;
        while i < bytes.len() && bytes[i] != b'}' { i += 1; }
        let decls = String::from_utf8_lossy(&bytes[decl_start..i.min(bytes.len())]).to_string();
        if i < bytes.len() { i += 1; } // skip '}'

        for sel_part in sel.split(',') {
            if let Some((selector, spec)) = parse_selector(sel_part) {
                let decl = parse_decls(&decls);
                rules.push(Rule { selector, decls: decl, specificity: spec, order });
                order += 1;
            }
        }
    }
    rules
}

fn spec_gt(a: (u8,u8,u8), b: (u8,u8,u8)) -> bool {
    if a.0 != b.0 { return a.0 > b.0; }
    if a.1 != b.1 { return a.1 > b.1; }
    a.2 > b.2
}

pub fn apply_rules(tag: &str, id: Option<&str>, classes: &[&str], rules: &[Rule], inline: &Style) -> Style {
    let tag = tag.to_ascii_lowercase();
    let id_l = id.map(|s| s.to_ascii_lowercase());
    let mut best_display: Option<(Display,(u8,u8,u8),u32)> = None;
    let mut best_color: Option<(u32,(u8,u8,u8),u32)> = None;
    let mut best_bold: Option<(bool,(u8,u8,u8),u32)> = None;

    for r in rules {
        let m = match &r.selector {
            Selector::Tag(t) => &tag == t,
            Selector::Class(c) => classes.iter().any(|cl| cl.to_ascii_lowercase() == *c),
            Selector::Id(i) => id_l.as_deref() == Some(i.as_str()),
            Selector::TagClass(t,c) => &tag == t && classes.iter().any(|cl| cl.to_ascii_lowercase() == *c),
            Selector::TagId(t,i) => &tag == t && id_l.as_deref() == Some(i.as_str()),
        };
        if !m { continue; }

        let spec = r.specificity;
        let ord = r.order;

        if let Some(d) = &r.decls.display {
            match &best_display {
                None => best_display = Some((d.clone(), spec, ord)),
                Some((_, bs, bo)) => {
                    if spec_gt(spec, *bs) || (spec == *bs && ord >= *bo) {
                        best_display = Some((d.clone(), spec, ord));
                    }
                }
            }
        }
        if let Some(c) = r.decls.color {
            match &best_color {
                None => best_color = Some((c, spec, ord)),
                Some((_, bs, bo)) => {
                    if spec_gt(spec, *bs) || (spec == *bs && ord >= *bo) {
                        best_color = Some((c, spec, ord));
                    }
                }
            }
        }
        if let Some(b) = r.decls.font_weight_bold {
            match &best_bold {
                None => best_bold = Some((b, spec, ord)),
                Some((_, bs, bo)) => {
                    if spec_gt(spec, *bs) || (spec == *bs && ord >= *bo) {
                        best_bold = Some((b, spec, ord));
                    }
                }
            }
        }
    }

    // inline overrides everything
    let mut out = Style::default();
    if let Some((d,_,_)) = best_display { out.display = Some(d); }
    if let Some((c,_,_)) = best_color { out.color = Some(c); }
    if let Some((b,_,_)) = best_bold { out.font_weight_bold = Some(b); }

    if inline.display.is_some() { out.display = inline.display.clone(); }
    if inline.color.is_some() { out.color = inline.color; }
    if inline.font_weight_bold.is_some() { out.font_weight_bold = inline.font_weight_bold; }

    out
}

pub fn parse_inline_style(s: &str) -> Style {
    // reuse decl parsing
    parse_decls(s)
}
