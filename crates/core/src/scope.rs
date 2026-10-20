//! Scope index.
//!
//! The grant of a session is a set of root node ids. A node may be read when
//! it is one of the roots or a descendant of one of them. The daemon tracks
//! the parent relation of every node it has observed, so membership is
//! checked locally as well as inside the plugin. Both sides must agree
//! (`FR03`, double enforcement).

use std::collections::{HashMap, HashSet};

/// In-memory index of node parents.
#[derive(Debug, Default, Clone)]
pub struct NodeIndex {
    parents: HashMap<String, String>,
    children: HashMap<String, HashSet<String>>,
}

impl NodeIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `node_id` is a child of `parent_id`.
    pub fn insert(&mut self, node_id: impl Into<String>, parent_id: impl Into<String>) {
        let node_id = node_id.into();
        let parent_id = parent_id.into();
        if node_id == parent_id {
            return;
        }
        self.children
            .entry(parent_id.clone())
            .or_default()
            .insert(node_id.clone());
        self.parents.insert(node_id, parent_id);
    }

    /// Parent of a node, when known.
    pub fn parent_of(&self, node_id: &str) -> Option<&str> {
        self.parents.get(node_id).map(String::as_str)
    }

    /// Direct children of a node.
    pub fn children_of(&self, node_id: &str) -> Vec<&str> {
        self.children
            .get(node_id)
            .map(|set| set.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Whether `node_id` is `root_id` itself or a descendant of it.
    ///
    /// The walk is bounded so a corrupted index cannot hang the daemon.
    pub fn contains(&self, root_id: &str, node_id: &str) -> bool {
        if root_id == node_id {
            return true;
        }
        let mut current = node_id;
        let mut hops = 0usize;
        let max_hops = self.parents.len() + 1;
        while let Some(parent) = self.parent_of(current) {
            if parent == root_id {
                return true;
            }
            hops += 1;
            if hops > max_hops {
                return false;
            }
            current = parent;
        }
        false
    }

    /// Whether a node is inside any of the roots.
    pub fn in_any_scope(&self, roots: &[String], node_id: &str) -> bool {
        roots.iter().any(|root| self.contains(root, node_id))
    }

    /// Number of indexed nodes.
    pub fn len(&self) -> usize {
        self.parents.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    /// Remove every relation of one root and its descendants.
    pub fn remove_scope(&mut self, root_id: &str) {
        let mut to_remove = vec![root_id.to_string()];
        let mut index = 0usize;
        while index < to_remove.len() {
            let current = to_remove[index].clone();
            for child in self.children_of(&current) {
                to_remove.push(child.to_string());
            }
            index += 1;
        }
        for node in to_remove {
            self.children.remove(&node);
            if let Some(parent) = self.parents.remove(&node) {
                if let Some(siblings) = self.children.get_mut(&parent) {
                    siblings.remove(&node);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NodeIndex {
        let mut index = NodeIndex::new();
        index.insert("1:3", "1:2");
        index.insert("1:4", "1:3");
        index.insert("2:1", "2:0");
        index
    }

    #[test]
    fn descendants_are_inside_the_root() {
        let index = sample();
        assert!(index.contains("1:2", "1:2"));
        assert!(index.contains("1:2", "1:3"));
        assert!(index.contains("1:2", "1:4"));
        assert!(!index.contains("1:2", "2:1"));
    }

    #[test]
    fn siblings_of_another_root_are_outside() {
        let index = sample();
        assert!(!index.in_any_scope(&["1:2".to_string()], "2:1"));
        assert!(index.in_any_scope(&["1:2".to_string(), "2:0".to_string()], "2:1"));
    }

    #[test]
    fn removing_a_scope_drops_its_descendants() {
        let mut index = sample();
        index.remove_scope("1:2");
        assert!(!index.contains("1:2", "1:4"));
        assert!(index.contains("2:0", "2:1"));
    }

    #[test]
    fn unknown_nodes_are_not_claimed() {
        let index = sample();
        assert!(!index.contains("1:2", "9:9"));
    }
}
