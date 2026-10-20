//! Plugin payload contract.
//!
//! The TypeScript plugin sends design data to the daemon as JSON. This module
//! is the Rust view of that wire shape, so the daemon and the plugin cannot
//! drift apart silently. The plugin is the only producer of these types.
//!
//! The shape is intentionally permissive: fields the platform did not expose
//! are absent, and unknown node types are preserved verbatim instead of being
//! dropped.

use std::collections::BTreeMap;

use figtether_model::capabilities::CapabilityState;
use serde::{Deserialize, Serialize};

/// A node as reported by the plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginNode {
    /// Figma node id.
    pub id: String,
    /// Parent node id, when the parent is inside the traversed scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Node type, for example `FRAME`. Unknown types are preserved.
    pub node_type: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Whether the node is visible.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Whether the node is locked.
    #[serde(default)]
    pub locked: bool,
    /// Node ids of the children inside the traversed scope.
    #[serde(default)]
    pub children: Vec<String>,
    /// Geometry fields, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<PluginGeometry>,
    /// Auto layout fields, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PluginLayout>,
    /// Text fields, when the node is a text node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<PluginText>,
    /// Fills.
    #[serde(default)]
    pub fills: Vec<PluginPaint>,
    /// Strokes.
    #[serde(default)]
    pub strokes: Vec<PluginPaint>,
    /// Effects.
    #[serde(default)]
    pub effects: Vec<PluginEffect>,
    /// Component reference, when the node is an instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<PluginComponent>,
    /// Variable references used by the node.
    #[serde(default)]
    pub variables: Vec<PluginVariable>,
    /// Style references used by the node.
    #[serde(default)]
    pub styles: Vec<PluginVariable>,
    /// Fields the plugin could not obtain, with the reason.
    #[serde(default)]
    pub unavailable_fields: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Geometry fields of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginGeometry {
    /// Transform relative to the parent.
    pub local_transform: [f64; 4],
    /// Transform relative to the canvas.
    pub absolute_transform: [f64; 4],
    /// Bounding box.
    pub bounding_box: [f64; 4],
    /// Rotation in degrees.
    #[serde(default)]
    pub rotation: f64,
    /// Horizontal constraint.
    #[serde(default)]
    pub constraints_horizontal: String,
    /// Vertical constraint.
    #[serde(default)]
    pub constraints_vertical: String,
    /// Coordinate space the numbers are expressed in.
    #[serde(default = "default_coordinate_space")]
    pub coordinate_space: String,
}

fn default_coordinate_space() -> String {
    "page".to_string()
}

/// Auto layout fields of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLayout {
    /// Whether auto layout is enabled.
    pub auto_layout: bool,
    /// Primary axis.
    #[serde(default)]
    pub axis: String,
    /// Gap between children.
    #[serde(default)]
    pub gap: f64,
    /// Padding tuple: top, right, bottom, left.
    #[serde(default)]
    pub padding: [f64; 4],
    /// Primary axis alignment.
    #[serde(default)]
    pub primary_axis_align: String,
    /// Counter axis alignment.
    #[serde(default)]
    pub counter_axis_align: String,
    /// Sizing mode: `FIXED`, `HUG`, or `FILL`.
    #[serde(default)]
    pub sizing: String,
    /// Whether text wraps.
    #[serde(default)]
    pub wraps: bool,
}

/// Text fields of a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginText {
    /// Full text.
    pub text: String,
    /// Styled runs.
    #[serde(default)]
    pub runs: Vec<PluginTextRun>,
    /// Text alignment.
    #[serde(default)]
    pub text_align: String,
    /// Whether the text is truncated.
    #[serde(default)]
    pub truncated: bool,
}

/// One styled run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginTextRun {
    /// Start character offset.
    pub start: usize,
    /// End character offset.
    pub end: usize,
    /// Font family.
    pub font_family: String,
    /// Font style.
    pub font_style: String,
    /// Font weight.
    pub font_weight: u16,
    /// Font size.
    pub font_size: f64,
    /// Line height.
    pub line_height: f64,
    /// Letter spacing.
    pub letter_spacing: f64,
    /// Whether the font is available in the editor runtime.
    #[serde(default = "default_true")]
    pub font_available: bool,
}

/// A fill or stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginPaint {
    /// Kind: `SOLID`, `GRADIENT`, `IMAGE`, or `UNKNOWN`.
    pub kind: String,
    /// Colour channels, when the paint is solid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 4]>,
    /// Gradient stops.
    #[serde(default)]
    pub gradient_stops: Vec<PluginGradientStop>,
    /// Opacity.
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Whether the paint is visible.
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_opacity() -> f64 {
    1.0
}

/// One gradient stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginGradientStop {
    /// Position of the stop.
    pub position: f64,
    /// Colour at the stop.
    pub color: [f64; 4],
}

/// One effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginEffect {
    /// Kind of the effect.
    pub kind: String,
    /// Whether the effect is visible.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Radius of the blur or shadow.
    #[serde(default)]
    pub radius: f64,
    /// Shadow colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 4]>,
}

/// Component reference of an instance node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginComponent {
    /// Instance node id.
    pub instance_id: String,
    /// Main component id, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_component_id: Option<String>,
    /// Variant properties.
    #[serde(default)]
    pub variant_properties: Vec<PluginVariantProperty>,
}

/// One variant property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginVariantProperty {
    /// Property name.
    pub name: String,
    /// Property value.
    pub value: String,
}

/// A variable or style reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginVariable {
    /// Id of the variable or style.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Collection, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// Mode, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Value, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Alias target, when the variable aliases another variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_target: Option<String>,
    /// Whether the reference resolved.
    #[serde(default)]
    pub resolved: bool,
    /// Reason for an unresolved reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_reason: Option<String>,
}

/// A document as reported by the plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginDocument {
    /// Human visible document label. Never an authorization key.
    #[serde(default)]
    pub document_label: String,
    /// Name of the page that was read.
    #[serde(default)]
    pub page_name: String,
    /// Nodes in the traversed scope.
    #[serde(default)]
    pub nodes: Vec<PluginNode>,
    /// Capabilities detected in the plugin runtime.
    #[serde(default)]
    pub capabilities: BTreeMap<String, CapabilityState>,
}

/// The result of an asset export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginExport {
    /// Node that was exported.
    pub node_id: String,
    /// MIME type of the export.
    pub mime: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Raw bytes of the export.
    #[serde(with = "crate::base64_bytes")]
    pub bytes: Vec<u8>,
    /// Hash of the export bytes.
    pub hash: String,
}

/// Capabilities reported by the plugin runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Probed capabilities.
    pub capabilities: BTreeMap<String, CapabilityState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_node_types_are_kept() {
        let node: PluginNode = serde_json::from_value(serde_json::json!({
            "id": "1:2",
            "node_type": "FUTURE_THING",
            "name": "mystery"
        }))
        .expect("parses");
        assert_eq!(node.node_type, "FUTURE_THING");
        assert!(node.visible, "visible defaults to true");
    }

    #[test]
    fn plugin_document_tolerates_minimal_payloads() {
        let doc: PluginDocument = serde_json::from_value(serde_json::json!({})).expect("parses");
        assert!(doc.nodes.is_empty());
        assert!(doc.document_label.is_empty());
    }

    #[test]
    fn export_bytes_use_base64() {
        let export: PluginExport = serde_json::from_value(serde_json::json!({
            "node_id": "1:2",
            "mime": "image/png",
            "width": 4,
            "height": 4,
            "bytes": "AAECAw==",
            "hash": "sha256:abc"
        }))
        .expect("parses");
        assert_eq!(export.bytes, vec![0, 1, 2, 3]);
    }
}
