//! The fixture corpus of the harness.
//!
//! The corpus of the repository lives under `fixtures/`. The harness reads
//! the committed files, so every acceptance flow runs against the same
//! documents as the acceptance catalog and the benchmark plan. The inline
//! generator stays available as a fallback for the two scenarios the
//! committed corpus cannot serve: a scenario that mutates a document while
//! the plugin serves it, and a scenario that needs a document of an exact
//! size.
//!
//! The committed documents are plugin payload documents, the shape of
//! `PluginDocument` in `crates/protocol/src/plugin.rs`. The harness keeps
//! the child lists of the file, so a fixture that carries a broken child
//! reference or a parent cycle keeps it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use figtether_core::scope::NodeIndex;
use figtether_protocol::plugin::{
    PluginDocument, PluginGradientStop, PluginLayout, PluginNode, PluginPaint, PluginText,
};
use serde_json::Value;

/// Node id of the root of the generated corpus.
pub const ROOT_ID: &str = "1:2";

/// The identifiers of the committed corpus.
pub const COMMITTED_IDS: [&str; 6] = [
    "FX-SMALL",
    "FX-MEDIUM",
    "FX-LARGE",
    "FX-CROSSPAGE",
    "FX-MIXED",
    "FX-ADVERSARIAL",
];

/// A document the fake plugin serves.
#[derive(Debug, Clone)]
pub struct FixtureCorpus {
    document_label: String,
    page_name: String,
    root_id: String,
    nodes: Vec<PluginNode>,
}

impl FixtureCorpus {
    /// `FX-SMALL`: the baseline document of the committed corpus.
    ///
    /// One hundred nodes: simple frames, text nodes, solid fills, strokes, an
    /// image fill, a variant instance, an auto layout, a drop shadow and
    /// geometry.
    pub fn small() -> Self {
        Self::committed("FX-SMALL").expect("the committed fixture exists")
    }

    /// `FX-MIXED`: the attribute checklist of the committed corpus.
    ///
    /// The ground truth of the normalization: unknown node types, unresolved
    /// references, gradients, masks, image fills, hidden and locked nodes and
    /// geometry edge cases.
    pub fn mixed() -> Self {
        Self::committed("FX-MIXED").expect("the committed fixture exists")
    }

    /// `FX-ADVERSARIAL`: the hostile input of the committed corpus.
    ///
    /// Missing references, a parent cycle, a very deep chain, an over budget
    /// descendant claim and a text node with empty runs.
    pub fn adversarial() -> Self {
        Self::committed("FX-ADVERSARIAL").expect("the committed fixture exists")
    }

    /// `FX-MEDIUM`: the steady state document of the committed corpus.
    pub fn medium() -> Self {
        Self::committed("FX-MEDIUM").expect("the committed fixture exists")
    }

    /// Load a fixture of the committed corpus by its identifier.
    pub fn committed(id: &str) -> Result<Self, String> {
        let path = committed_fixture_path(id)?;
        let raw = std::fs::read_to_string(&path)
            .map_err(|err| format!("the fixture {} could not be read: {err}", path.display()))?;
        let document: PluginDocument = serde_json::from_str(&raw).map_err(|err| {
            format!(
                "the fixture {} is not a plugin document: {err}",
                path.display()
            )
        })?;
        Ok(Self::from_document(document))
    }

    /// The directory of the committed corpus.
    pub fn fixtures_dir() -> PathBuf {
        workspace_root().join("fixtures")
    }

    /// `FX-MEDIUM`: a generated document of one thousand nodes.
    ///
    /// The inline generator is the fallback for a scenario that needs a
    /// document of an exact size without touching the committed corpus.
    pub fn generated_medium() -> Self {
        Self::tree("FX-MEDIUM", ROOT_ID, 1000, "medium")
    }

    /// `FX-MIXED`: a generated document with the attribute checklist.
    ///
    /// The inline generator is the fallback for a scenario that mutates the
    /// document while the plugin serves it.
    pub fn generated_mixed() -> Self {
        let mut corpus = Self::tree("FX-MIXED", ROOT_ID, 300, "mixed");
        corpus.enrich();
        corpus
    }

    /// A generated document large enough to force pagination.
    pub fn generated_large(node_count: usize) -> Self {
        Self::tree("FX-LARGE", ROOT_ID, node_count.max(2), "large")
    }

    /// A corpus with an explicit label.
    ///
    /// Two corpora built with different labels share every node id but differ
    /// in content, which is what the session isolation scenario needs. The
    /// inline generator is the fallback for that scenario, because it needs
    /// two documents that share every node id.
    pub fn labelled(label: &str, count: usize) -> Self {
        Self::tree(label, ROOT_ID, count, label)
    }

    /// The first of two documents that share every node id.
    ///
    /// `file_a` and `file_b` model two tabs of two different files that
    /// happen to use the same node ids. The content differs, so a session
    /// that leaks across the boundary is visible in the payload.
    pub fn file_a() -> Self {
        Self::labelled("FX-FILE-A", 40)
    }

    /// The second of two documents that share every node id.
    pub fn file_b() -> Self {
        Self::labelled("FX-FILE-B", 40)
    }

    /// Build a corpus from an explicit node list.
    ///
    /// The child lists are rebuilt from the parent relations.
    pub fn from_nodes(document_label: &str, page_name: &str, nodes: Vec<PluginNode>) -> Self {
        let mut corpus = Self {
            document_label: document_label.to_string(),
            page_name: page_name.to_string(),
            root_id: String::new(),
            nodes,
        };
        corpus.rebuild_children();
        corpus.rebuild_root();
        corpus
    }

    /// Build a corpus from a plugin document.
    ///
    /// The child lists of the document are kept exactly as they are, so a
    /// fixture that carries a broken reference keeps it.
    pub fn from_document(document: PluginDocument) -> Self {
        let root_id = document
            .nodes
            .iter()
            .find(|node| node.parent_id.is_none())
            .map(|node| node.id.clone())
            .unwrap_or_default();
        Self {
            document_label: document.document_label,
            page_name: document.page_name,
            root_id,
            nodes: document.nodes,
        }
    }

    fn tree(label: &str, root_id: &str, count: usize, variant: &str) -> Self {
        let mut nodes: Vec<PluginNode> = Vec::with_capacity(count);
        for index in 0..count {
            let id = if index == 0 {
                root_id.to_string()
            } else {
                node_id(index)
            };
            let parent = if index == 0 {
                None
            } else {
                Some(node_id((index - 1) / 3))
            };
            let is_text = index % 4 == 3;
            let node_type = if is_text { "TEXT" } else { "FRAME" };
            let name = format!("{label} {variant} node {index}");
            let text = if is_text {
                Some(format!("{label} {variant} value {index}"))
            } else {
                None
            };
            nodes.push(make_node(id, parent, node_type, name, text));
        }
        let mut corpus = Self {
            document_label: label.to_string(),
            page_name: "Page 1".to_string(),
            root_id: root_id.to_string(),
            nodes,
        };
        corpus.rebuild_children();
        corpus
    }

    /// Add the attributes of the fixture attribute checklist.
    fn enrich(&mut self) {
        for (index, node) in self.nodes.iter_mut().enumerate() {
            if index % 4 == 3 {
                node.layout = Some(PluginLayout {
                    auto_layout: true,
                    axis: "VERTICAL".to_string(),
                    gap: 8.0,
                    padding: [4.0, 8.0, 4.0, 8.0],
                    primary_axis_align: "MIN".to_string(),
                    counter_axis_align: "MIN".to_string(),
                    sizing: "FILL".to_string(),
                    wraps: true,
                });
                node.fills.push(PluginPaint {
                    kind: "GRADIENT".to_string(),
                    color: None,
                    gradient_stops: vec![
                        PluginGradientStop {
                            position: 0.0,
                            color: [0.0, 0.0, 0.0, 1.0],
                        },
                        PluginGradientStop {
                            position: 1.0,
                            color: [1.0, 1.0, 1.0, 1.0],
                        },
                    ],
                    opacity: 1.0,
                    visible: true,
                });
            }
            if index % 7 == 0 {
                node.unavailable_fields.push("variables".to_string());
            }
        }
    }

    fn rebuild_children(&mut self) {
        let mut children: Vec<Vec<String>> = vec![Vec::new(); self.nodes.len()];
        let index_of = |id: &str| self.nodes.iter().position(|node| node.id == id);
        for (position, node) in self.nodes.iter().enumerate() {
            if let Some(parent_id) = node.parent_id.as_ref() {
                if let Some(parent_position) = index_of(parent_id) {
                    children[parent_position].push(node.id.clone());
                }
            }
            let _ = position;
        }
        for (position, node) in self.nodes.iter_mut().enumerate() {
            node.children = children[position].clone();
        }
    }

    fn rebuild_root(&mut self) {
        if self.root_id.is_empty() {
            self.root_id = self
                .nodes
                .iter()
                .find(|node| node.parent_id.is_none())
                .map(|node| node.id.clone())
                .unwrap_or_default();
        }
    }

    /// Human visible label of the document. Never an authorization key.
    pub fn document_label(&self) -> &str {
        &self.document_label
    }

    /// Name of the page the nodes live on.
    pub fn page_name(&self) -> &str {
        &self.page_name
    }

    /// The root node id of the corpus.
    pub fn root_id(&self) -> &str {
        &self.root_id
    }

    /// Every node of the corpus.
    pub fn nodes(&self) -> &[PluginNode] {
        &self.nodes
    }

    /// Number of nodes in the corpus.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the corpus is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Stable hash of the corpus content.
    ///
    /// The acceptance test `AT18` compares this hash before and after a read
    /// and export pass to prove that nothing in the read path mutates the
    /// document.
    pub fn hash(&self) -> String {
        let encoded = serde_json::to_vec(&self.nodes).unwrap_or_default();
        figtether_protocol::sha256_hex(&encoded)
    }

    /// Look up one node.
    pub fn node(&self, node_id: &str) -> Option<&PluginNode> {
        self.nodes.iter().find(|node| node.id == node_id)
    }

    /// The first text node of the corpus, when it has one.
    pub fn first_text_node(&self) -> Option<&str> {
        self.nodes
            .iter()
            .find(|node| node.node_type == "TEXT")
            .map(|node| node.id.as_str())
    }

    /// Change the text of one node, the way an editor edit would.
    ///
    /// Returns `false` when the node does not exist.
    pub fn apply_edit(&mut self, node_id: &str, new_text: &str) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == node_id) else {
            return false;
        };
        node.name = new_text.to_string();
        if let Some(text) = node.text.as_mut() {
            text.text = new_text.to_string();
        }
        true
    }

    /// The parent relations of the corpus.
    pub fn node_index(&self) -> NodeIndex {
        let mut index = NodeIndex::new();
        for node in &self.nodes {
            if let Some(parent_id) = node.parent_id.as_ref() {
                index.insert(node.id.clone(), parent_id.clone());
            }
        }
        index
    }

    /// The nodes below `root_id`, breadth first, limited to `max_depth`.
    ///
    /// The walk uses the parent relations of the corpus, so it visits every
    /// node at most once per level and the depth bound keeps a cycle in the
    /// parent relation from turning the walk into an endless loop.
    pub fn descendants(&self, root_id: &str, max_depth: u32) -> Vec<PluginNode> {
        let mut by_parent: BTreeMap<&str, Vec<&PluginNode>> = BTreeMap::new();
        for node in &self.nodes {
            if let Some(parent_id) = node.parent_id.as_deref() {
                by_parent.entry(parent_id).or_default().push(node);
            }
        }
        let mut result = Vec::new();
        let mut frontier = vec![root_id.to_string()];
        let mut depth = 0u32;
        while !frontier.is_empty() && depth <= max_depth {
            let mut next = Vec::new();
            for id in &frontier {
                let Some(children) = by_parent.get(id.as_str()) else {
                    continue;
                };
                for child in children {
                    next.push(child.id.clone());
                    result.push((*child).clone());
                }
            }
            frontier = next;
            depth += 1;
        }
        result
    }

    /// The nodes with the given ids, in the order of the request.
    pub fn nodes_by_ids(&self, node_ids: &[String]) -> Vec<PluginNode> {
        node_ids
            .iter()
            .filter_map(|id| self.node(id).cloned())
            .collect()
    }

    /// The document as the plugin reports it.
    pub fn to_document(&self) -> PluginDocument {
        PluginDocument {
            document_label: self.document_label.clone(),
            page_name: self.page_name.clone(),
            nodes: self.nodes.clone(),
            capabilities: BTreeMap::new(),
        }
    }
}

/// The workspace root of the repository.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("the crate lives in the workspace")
        .to_path_buf()
}

/// The path of a fixture of the committed corpus.
///
/// The manifest of the corpus names the file of every identifier, so the
/// harness never guesses a file name.
pub fn committed_fixture_path(id: &str) -> Result<PathBuf, String> {
    let directory = FixtureCorpus::fixtures_dir();
    let manifest_path = directory.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "the manifest {} could not be read: {err}",
            manifest_path.display()
        )
    })?;
    let manifest: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("the manifest is not valid JSON: {err}"))?;
    let entry = manifest
        .get("fixtures")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        })
        .ok_or_else(|| format!("the manifest names no fixture {id}"))?;
    let file = entry
        .get("file")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the manifest entry of {id} names no file"))?;
    Ok(directory.join(file))
}

fn node_id(index: usize) -> String {
    format!("{}:{}", index / 60 + 1, index + 2)
}

fn make_node(
    id: String,
    parent_id: Option<String>,
    node_type: &str,
    name: String,
    text: Option<String>,
) -> PluginNode {
    PluginNode {
        id,
        parent_id,
        node_type: node_type.to_string(),
        name,
        visible: true,
        locked: false,
        children: Vec::new(),
        geometry: None,
        layout: None,
        text: text.map(|value| PluginText {
            text: value,
            runs: Vec::new(),
            text_align: "LEFT".to_string(),
            truncated: false,
        }),
        fills: Vec::new(),
        strokes: Vec::new(),
        effects: Vec::new(),
        component: None,
        variables: Vec::new(),
        styles: Vec::new(),
        unavailable_fields: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_corpus_is_read_from_the_repository() {
        let corpus = FixtureCorpus::small();
        assert_eq!(corpus.document_label(), "FigTether fixture FX-SMALL");
        assert_eq!(corpus.len(), 100);
        assert_eq!(corpus.root_id(), "1:0");
        assert!(corpus.node("1:0").is_some());
    }

    #[test]
    fn the_committed_mixed_corpus_carries_the_unknown_shapes() {
        let corpus = FixtureCorpus::mixed();
        assert_eq!(corpus.len(), 201);
        assert_eq!(corpus.root_id(), "5:0");
        assert!(
            corpus
                .nodes()
                .iter()
                .any(|node| node.node_type == "WIDGET_FUTURE_9"),
            "the mixed fixture keeps the unknown node type"
        );
    }

    #[test]
    fn the_committed_adversarial_corpus_keeps_its_broken_references() {
        let corpus = FixtureCorpus::adversarial();
        assert_eq!(corpus.len(), 312);
        assert_eq!(corpus.root_id(), "9:0");
        assert!(
            corpus.node("9:603").is_some(),
            "the node with the missing parent stays in the corpus"
        );
    }

    #[test]
    fn every_committed_identifier_names_a_file() {
        for id in COMMITTED_IDS {
            let path = committed_fixture_path(id).expect("the manifest names the file");
            assert!(path.is_file(), "{} is missing", path.display());
        }
    }

    #[test]
    fn the_generated_corpus_is_deterministic() {
        let first = FixtureCorpus::generated_medium();
        let second = FixtureCorpus::generated_medium();
        assert_eq!(first.hash(), second.hash());
        assert_eq!(first.len(), 1000);
        assert_eq!(first.root_id(), ROOT_ID);
    }

    #[test]
    fn the_two_files_share_node_ids_but_not_content() {
        let a = FixtureCorpus::file_a();
        let b = FixtureCorpus::file_b();
        assert_ne!(a.hash(), b.hash());
        assert_eq!(
            a.node("1:3").map(|node| node.id.clone()),
            b.node("1:3").map(|node| node.id.clone())
        );
        assert_ne!(
            a.node("1:3").map(|node| node.name.clone()),
            b.node("1:3").map(|node| node.name.clone())
        );
    }

    #[test]
    fn descendants_stay_inside_the_root() {
        let corpus = FixtureCorpus::small();
        let descendants = corpus.descendants(corpus.root_id(), 32);
        assert!(!descendants.is_empty());
        let index = corpus.node_index();
        assert!(descendants
            .iter()
            .all(|node| index.contains(corpus.root_id(), &node.id)));
    }
}
