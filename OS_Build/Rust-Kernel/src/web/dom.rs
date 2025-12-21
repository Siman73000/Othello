#![allow(dead_code)]
extern crate alloc;

use alloc::{string::String, vec::Vec};

#[derive(Clone, Debug)]
pub enum NodeKind {
    Document,
    Element(ElementData),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct ElementData {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Document {
    pub fn new() -> Self {
        let mut nodes = Vec::new();
        nodes.push(Node { kind: NodeKind::Document, parent: None, children: Vec::new() });
        Self { nodes, root: 0 }
    }

    pub fn push(&mut self, parent: usize, kind: NodeKind) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(Node { kind, parent: Some(parent), children: Vec::new() });
        self.nodes[parent].children.push(idx);
        idx
    }

    pub fn element_tag(&self, idx: usize) -> Option<&str> {
        match &self.nodes[idx].kind {
            NodeKind::Element(e) => Some(e.tag.as_str()),
            _ => None,
        }
    }

    pub fn element_attr(&self, idx: usize, key: &str) -> Option<&str> {
        match &self.nodes[idx].kind {
            NodeKind::Element(e) => e.attrs.iter().find(|(k,_)| k == key).map(|(_,v)| v.as_str()),
            _ => None,
        }
    }

    pub fn element_classes(&self, idx: usize) -> Vec<&str> {
        if let Some(c) = self.element_attr(idx, "class") {
            c.split_whitespace().collect()
        } else {
            Vec::new()
        }
    }

    pub fn element_id(&self, idx: usize) -> Option<&str> {
        self.element_attr(idx, "id")
    }
}
