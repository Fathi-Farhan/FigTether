//! Deterministic fixtures.
//!
//! The benchmark plan requires a fixture set that is pinned to a commit and
//! content identical across a run. This module builds that corpus in process
//! from a fixed seed, so a run needs no external file, no network, and no
//! editor session, and two runs see byte identical inputs.
//!
//! The fixtures imitate the shape of a Figma document as the plugin reports
//! it: a tree of frames, groups, text nodes and vectors with geometry, auto
//! layout, fills, strokes, effects, component and variable references, plus
//! the fields the platform did not expose. One node carries an unknown node
//! type so the "unknown types are preserved" rule is exercised by the data
//! itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use figtether_model::capabilities::CapabilityState;
use figtether_protocol::plugin::{
    PluginComponent, PluginDocument, PluginEffect, PluginGeometry, PluginGradientStop,
    PluginLayout, PluginNode, PluginPaint, PluginText, PluginTextRun, PluginVariable,
};
use sha2::{Digest, Sha256};

/// Number of children every fixture node has, except the leaves.
const BRANCHING: usize = 4;

/// Node types the generator cycles through.
const NODE_TYPES: [&str; 8] = [
    "FRAME",
    "GROUP",
    "TEXT",
    "RECTANGLE",
    "VECTOR",
    "ELLIPSE",
    "COMPONENT",
    "INSTANCE",
];

/// A small deterministic pseudo random generator.
///
/// SplitMix64 is used because it is short, has no dependency, and produces the
/// same sequence on every platform and every Rust release, which keeps the
/// fixture hashes stable over time.
#[derive(Debug, Clone)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    /// Create a generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next value in `0.0..1.0`.
    pub fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    /// Next value in `low..high`.
    pub fn next_range(&mut self, low: usize, high: usize) -> usize {
        if high <= low {
            return low;
        }
        low + ((self.next_u64() % (high - low) as u64) as usize)
    }

    /// Whether the next draw passes the given probability.
    pub fn chance(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }
}

/// One pinned fixture.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Stable fixture name, for example `small-100`.
    pub name: String,
    /// Seed the fixture was generated from.
    pub seed: u64,
    /// Number of nodes in the fixture.
    pub node_count: usize,
    /// The document as the plugin would report it.
    pub document: PluginDocument,
    /// Content hash of the document.
    pub hash: String,
}

impl Fixture {
    /// Node ids in document order.
    pub fn node_ids(&self) -> Vec<String> {
        self.document
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect()
    }

    /// Node ids split into pages of at most `page_size` entries.
    ///
    /// The page size never exceeds the batch limit of the resource contract,
    /// so a page is always a legal request.
    pub fn pages(&self, page_size: usize) -> Vec<Vec<String>> {
        let ids = self.node_ids();
        if page_size == 0 {
            return vec![ids];
        }
        ids.chunks(page_size).map(<[String]>::to_vec).collect()
    }

    /// Write the fixture to disk as canonical JSON and return the file path.
    pub fn materialize(&self, directory: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!("{}.json", self.name));
        let bytes = serde_json::to_vec(&self.document)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}

/// Hash a canonical JSON payload the same way the envelope does.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Write a fixture to a temporary directory and hash the written bytes.
///
/// The round trip proves that the fixture is byte stable on disk, which is
/// what the fixture hash of the release evidence refers to.
pub fn written_hash(fixture: &Fixture) -> std::io::Result<String> {
    let directory = tempfile::tempdir()?;
    let path = fixture.materialize(directory.path())?;
    let bytes = std::fs::read(path)?;
    Ok(content_hash(&bytes))
}

/// The small fixture of the benchmark plan: 100 nodes.
pub fn small_fixture() -> Fixture {
    build_fixture("small-100", 0x5EED_0001, 100)
}

/// The medium fixture of the benchmark plan: 1000 nodes.
pub fn medium_fixture() -> Fixture {
    build_fixture("medium-1000", 0x5EED_0002, 1000)
}

/// The export fixture of the benchmark plan: image fills, gradients, and text.
pub fn export_fixture() -> Fixture {
    build_fixture("export-64", 0x5EED_0003, 64)
}

/// Build a fixture with the given name, seed, and node count.
pub fn build_fixture(name: &str, seed: u64, node_count: usize) -> Fixture {
    let mut rng = DeterministicRng::new(seed);
    let ids: Vec<String> = (0..node_count).map(|index| format!("1:{index}")).collect();
    let mut children: Vec<Vec<String>> = vec![Vec::new(); node_count];
    for (index, id) in ids.iter().enumerate().skip(1) {
        let parent = (index - 1) / BRANCHING;
        children[parent].push(id.clone());
    }

    let mut nodes = Vec::with_capacity(node_count);
    for index in 0..node_count {
        let node_type = if index == 3 {
            "WIDGET_FUTURE_7".to_string()
        } else {
            NODE_TYPES[index % NODE_TYPES.len()].to_string()
        };
        let label = node_type.split('_').next().unwrap_or("NODE").to_lowercase();
        let name = format!("{label} {index}");
        let x = rng.next_range(0, 4000) as f64;
        let y = rng.next_range(0, 4000) as f64;
        let width = 24.0 + rng.next_range(0, 960) as f64;
        let height = 24.0 + rng.next_range(0, 960) as f64;

        let mut node = PluginNode {
            id: ids[index].clone(),
            parent_id: if index == 0 {
                None
            } else {
                Some(ids[(index - 1) / BRANCHING].clone())
            },
            node_type: node_type.clone(),
            name,
            visible: index % 7 != 0,
            locked: index % 11 == 0,
            children: children[index].clone(),
            geometry: Some(PluginGeometry {
                local_transform: [x, y, 1.0, 1.0],
                absolute_transform: [x, y, 1.0, 1.0],
                bounding_box: [x, y, width, height],
                rotation: 0.0,
                constraints_horizontal: "MIN".to_string(),
                constraints_vertical: "MIN".to_string(),
                coordinate_space: "page".to_string(),
            }),
            layout: None,
            text: None,
            fills: Vec::new(),
            strokes: Vec::new(),
            effects: Vec::new(),
            component: None,
            variables: Vec::new(),
            styles: Vec::new(),
            unavailable_fields: Vec::new(),
        };

        if matches!(
            node_type.as_str(),
            "FRAME" | "GROUP" | "COMPONENT" | "INSTANCE"
        ) {
            node.layout = Some(PluginLayout {
                auto_layout: index % 2 == 0,
                axis: if index % 2 == 0 {
                    "VERTICAL".to_string()
                } else {
                    "NONE".to_string()
                },
                gap: 8.0 + (index % 16) as f64,
                padding: [8.0, 8.0, 8.0, 8.0],
                primary_axis_align: "MIN".to_string(),
                counter_axis_align: "MIN".to_string(),
                sizing: "FIXED".to_string(),
                wraps: false,
            });
        }

        if node_type == "TEXT" {
            let content = format!("Text node {index}");
            node.text = Some(PluginText {
                text: content.clone(),
                runs: vec![PluginTextRun {
                    start: 0,
                    end: content.len(),
                    font_family: "Inter".to_string(),
                    font_style: "Regular".to_string(),
                    font_weight: 400,
                    font_size: 12.0 + (index % 24) as f64,
                    line_height: 16.0,
                    letter_spacing: 0.0,
                    font_available: index % 17 != 0,
                }],
                text_align: "LEFT".to_string(),
                truncated: false,
            });
        }

        node.fills.push(PluginPaint {
            kind: match index % 3 {
                0 => "SOLID".to_string(),
                1 => "GRADIENT".to_string(),
                _ => "IMAGE".to_string(),
            },
            color: Some([0.1 + ((index % 7) as f64) * 0.1, 0.2, 0.3, 1.0]),
            gradient_stops: if index % 3 == 1 {
                vec![
                    PluginGradientStop {
                        position: 0.0,
                        color: [0.0, 0.0, 0.0, 1.0],
                    },
                    PluginGradientStop {
                        position: 1.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                ]
            } else {
                Vec::new()
            },
            opacity: 1.0,
            visible: true,
        });

        if index % 3 == 0 {
            node.strokes.push(PluginPaint {
                kind: "SOLID".to_string(),
                color: Some([0.05, 0.05, 0.05, 1.0]),
                gradient_stops: Vec::new(),
                opacity: 1.0,
                visible: true,
            });
        }

        if index % 5 == 0 {
            node.effects.push(PluginEffect {
                kind: "DROP_SHADOW".to_string(),
                visible: true,
                radius: 4.0 + (index % 8) as f64,
                color: Some([0.0, 0.0, 0.0, 0.25]),
            });
        }

        if node_type == "INSTANCE" {
            node.component = Some(PluginComponent {
                instance_id: ids[index].clone(),
                main_component_id: Some(format!("9:{}", index % 32)),
                variant_properties: Vec::new(),
            });
        }

        if index % 4 == 0 {
            node.variables.push(PluginVariable {
                id: format!("VariableID:1:{index}"),
                name: format!("space/{}", index % 8),
                collection: Some("Spacing".to_string()),
                mode: Some("Default".to_string()),
                value: Some(serde_json::json!(8.0 + (index % 8) as f64)),
                alias_target: None,
                resolved: true,
                unresolved_reason: None,
            });
            node.styles.push(PluginVariable {
                id: format!("StyleID:2:{index}"),
                name: "Heading/Medium".to_string(),
                collection: Some("Typography".to_string()),
                mode: None,
                value: None,
                alias_target: None,
                resolved: index % 8 != 0,
                unresolved_reason: if index % 8 == 0 {
                    Some("style is not in the granted scope".to_string())
                } else {
                    None
                },
            });
        }

        if index % 13 == 0 {
            node.unavailable_fields.push("effects.radius".to_string());
        }

        nodes.push(node);
    }

    let mut capabilities: BTreeMap<String, CapabilityState> = BTreeMap::new();
    capabilities.insert("export_png".to_string(), CapabilityState::Available);
    capabilities.insert("export_svg".to_string(), CapabilityState::Available);
    capabilities.insert("variables".to_string(), CapabilityState::Available);
    capabilities.insert(
        "image_fill".to_string(),
        CapabilityState::Unavailable {
            reason: "offline domain fixture has no seat context".to_string(),
        },
    );

    let document = PluginDocument {
        document_label: format!("offline fixture {name}"),
        page_name: "Page 1".to_string(),
        nodes,
        capabilities,
    };
    let bytes = serde_json::to_vec(&document).unwrap_or_default();
    Fixture {
        name: name.to_string(),
        seed,
        node_count,
        document,
        hash: content_hash(&bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_deterministic() {
        let first = small_fixture();
        let second = small_fixture();
        assert_eq!(first.hash, second.hash);
        assert_eq!(first.node_count, 100);
        assert_eq!(medium_fixture().node_count, 1000);
    }

    #[test]
    fn the_tree_has_no_orphan_nodes() {
        let fixture = small_fixture();
        let known: Vec<&String> = fixture.document.nodes.iter().map(|node| &node.id).collect();
        for node in &fixture.document.nodes {
            if let Some(parent) = &node.parent_id {
                assert!(known.contains(&parent), "parent {parent} is missing");
            }
            for child in &node.children {
                assert!(known.contains(&child), "child {child} is missing");
            }
        }
    }

    #[test]
    fn an_unknown_node_type_is_preserved() {
        let fixture = small_fixture();
        let unknown = fixture
            .document
            .nodes
            .iter()
            .find(|node| node.node_type == "WIDGET_FUTURE_7");
        assert!(unknown.is_some());
    }

    #[test]
    fn pages_never_exceed_the_requested_size() {
        let fixture = medium_fixture();
        let pages = fixture.pages(50);
        assert!(pages.iter().all(|page| page.len() <= 50));
        assert_eq!(pages.iter().map(Vec::len).sum::<usize>(), 1000);
    }

    #[test]
    fn the_written_fixture_matches_its_hash() {
        let fixture = small_fixture();
        let written = written_hash(&fixture).expect("the fixture is written");
        assert_eq!(written, fixture.hash);
    }
}
