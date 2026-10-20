//! Normalized design model.
//!
//! The normalized model is the shape an agent consumes. It is intentionally
//! close to implementation semantics (layout, text runs, paints, component and
//! variable references) and deliberately explicit about what could not be
//! obtained: a field that the platform did not expose is reported as
//! unavailable with a reason, never as an empty value.
//!
//! `JSON_REST_V1` is an optional adapter input, never the internal model.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::AssetId;

/// Availability of one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FieldAvailability {
    /// The field is present and meaningful.
    Available,
    /// The field exists but the platform did not expose a value for it.
    Unavailable {
        /// Why the value is unavailable.
        reason: String,
    },
}

impl FieldAvailability {
    /// Whether the field carries a real value.
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Identity and tree position of a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeSummary {
    /// Figma node id, opaque string.
    pub id: String,
    /// Node type, for example `FRAME` or `TEXT`. Unknown types are preserved.
    pub node_type: String,
    /// Display name.
    pub name: String,
    /// Whether the node is visible.
    pub visible: bool,
    /// Whether the node is locked.
    pub locked: bool,
    /// Number of children inside the granted scope.
    pub child_count: u32,
}

/// A 2D point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

/// A rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// An affine transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transform {
    /// Horizontal translation.
    pub x: f64,
    /// Vertical translation.
    pub y: f64,
    /// Rotation in degrees.
    pub rotation: f64,
    /// Horizontal scale.
    pub scale_x: f64,
    /// Vertical scale.
    pub scale_y: f64,
}

/// Geometry of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Geometry {
    /// Transform relative to the parent.
    pub local_transform: Transform,
    /// Transform relative to the canvas.
    pub absolute_transform: Transform,
    /// Bounding box in the declared coordinate space.
    pub bounding_box: Rect,
    /// Layout constraints.
    pub constraints: Constraints,
    /// The coordinate system the numbers above are expressed in.
    pub coordinate_space: String,
}

/// Layout constraints of a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Constraints {
    /// Horizontal behaviour: `FIXED`, `SCALE`, `LEFT`, `RIGHT`, `CENTER`.
    pub horizontal: String,
    /// Vertical behaviour.
    pub vertical: String,
}

/// Sizing mode of a node inside auto layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SizingMode {
    /// Fixed size.
    Fixed,
    /// Hug contents.
    Hug,
    /// Fill the container.
    Fill,
}

/// Auto layout description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Layout {
    /// Whether auto layout is enabled.
    pub auto_layout: bool,
    /// Primary axis: `HORIZONTAL`, `VERTICAL`, or `NONE`.
    pub axis: String,
    /// Gap between children.
    pub gap: f64,
    /// Padding around children.
    pub padding: Padding,
    /// Counter axis alignment.
    pub primary_axis_align: String,
    /// Main axis alignment.
    pub counter_axis_align: String,
    /// Sizing of this node.
    pub sizing: SizingMode,
    /// Whether text wraps.
    pub wraps: bool,
}

/// Padding of a layout container.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Padding {
    /// Top padding.
    pub top: f64,
    /// Right padding.
    pub right: f64,
    /// Bottom padding.
    pub bottom: f64,
    /// Left padding.
    pub left: f64,
}

/// One styled run inside a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextRun {
    /// Start character offset.
    pub start: usize,
    /// End character offset.
    pub end: usize,
    /// Font family of the run.
    pub font_family: String,
    /// Font style of the run.
    pub font_style: String,
    /// Font weight of the run.
    pub font_weight: u16,
    /// Font size of the run.
    pub font_size: f64,
    /// Line height of the run.
    pub line_height: f64,
    /// Letter spacing of the run.
    pub letter_spacing: f64,
    /// Whether the run's font is available in this environment.
    pub font_available: bool,
}

/// Typography of a text node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Typography {
    /// Full text content.
    pub text: String,
    /// Styled runs covering the text.
    pub runs: Vec<TextRun>,
    /// Text alignment.
    pub text_align: String,
    /// Whether the text is truncated in the layout.
    pub truncated: bool,
}

/// Kind of a paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaintKind {
    /// Solid colour.
    Solid,
    /// Linear or radial gradient.
    Gradient,
    /// Image fill.
    Image,
    /// Any paint the platform did not classify.
    Unknown,
}

/// A colour with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Color {
    /// Red channel in `0.0..=1.0`.
    pub r: f64,
    /// Green channel in `0.0..=1.0`.
    pub g: f64,
    /// Blue channel in `0.0..=1.0`.
    pub b: f64,
    /// Alpha channel in `0.0..=1.0`.
    pub a: f64,
}

/// One gradient stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradientStop {
    /// Position of the stop in `0.0..=1.0`.
    pub position: f64,
    /// Colour at the stop.
    pub color: Color,
}

/// A fill or stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Paint {
    /// Kind of the paint.
    pub kind: PaintKind,
    /// Solid colour, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Gradient stops, when applicable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gradient_stops: Vec<GradientStop>,
    /// Opacity of the paint in `0.0..=1.0`.
    pub opacity: f64,
    /// Whether the paint is visible.
    pub visible: bool,
}

/// Kind of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    /// Drop shadow.
    DropShadow,
    /// Inner shadow.
    InnerShadow,
    /// Layer blur.
    Blur,
    /// Background blur.
    BackgroundBlur,
    /// Any effect the platform did not classify.
    Unknown,
}

/// One visual effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Effect {
    /// Kind of the effect.
    pub kind: EffectKind,
    /// Whether the effect is visible.
    pub visible: bool,
    /// Radius of the blur or shadow.
    pub radius: f64,
    /// Shadow colour, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// Reference to a component instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentRef {
    /// Id of the instance node.
    pub instance_id: String,
    /// Id of the main component.
    pub main_component_id: Option<String>,
    /// Variant properties of the instance.
    #[serde(default)]
    pub variant_properties: Vec<VariantProperty>,
    /// Whether the main component was resolved inside the granted scope.
    pub resolved: bool,
}

/// One variant property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VariantProperty {
    /// Property name.
    pub name: String,
    /// Property value.
    pub value: String,
}

/// Reference to a variable or style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VariableRef {
    /// Id of the variable or style.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Collection the variable belongs to.
    pub collection: Option<String>,
    /// Mode the value was read in.
    pub mode: Option<String>,
    /// Resolved value, when the reference could be resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Alias target, when the variable aliases another variable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias_target: Option<String>,
    /// Whether the reference resolved inside the granted scope.
    pub resolved: bool,
    /// Why the reference did not resolve, when it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_reason: Option<String>,
}

/// Export settings of an asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportSettings {
    /// Export format.
    pub format: String,
    /// Export scale factor.
    pub scale: f64,
    /// Whether the export uses the SVG suffix.
    pub svg_suffix: bool,
}

/// Reference to an exported asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssetRef {
    /// Opaque asset id.
    pub asset_id: AssetId,
    /// MIME type of the asset.
    pub mime: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Size in bytes.
    pub bytes: u64,
    /// Content hash.
    pub hash: String,
    /// Settings used for the export.
    pub export_settings: ExportSettings,
}

/// A normalized node with all available domains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedNode {
    /// Identity and tree position.
    pub summary: NodeSummary,
    /// Geometry, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    /// Auto layout, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    /// Typography, when the node is text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typography: Option<Typography>,
    /// Fills.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Paint>,
    /// Strokes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<Paint>,
    /// Effects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    /// Component reference, when the node is an instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentRef>,
    /// Variable references used by the node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VariableRef>,
    /// Style references used by the node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<VariableRef>,
    /// Exported assets of the node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<AssetRef>,
    /// Fields that could not be provided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_fields: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_node_types_are_preserved() {
        let summary = NodeSummary {
            id: "1:2".to_string(),
            node_type: "WIDGET_THAT_DOES_NOT_EXIST".to_string(),
            name: "mystery".to_string(),
            visible: true,
            locked: false,
            child_count: 0,
        };
        let json = serde_json::to_string(&summary).expect("serializes");
        assert!(json.contains("WIDGET_THAT_DOES_NOT_EXIST"));
    }

    #[test]
    fn unavailable_fields_carry_a_reason() {
        let state = FieldAvailability::Unavailable {
            reason: "seat does not expose variables".to_string(),
        };
        assert!(!state.is_available());
    }

    #[test]
    fn asset_ref_omits_empty_optionals() {
        let asset = AssetRef {
            asset_id: AssetId::new(),
            mime: "image/png".to_string(),
            width: 1024,
            height: 1024,
            bytes: 2048,
            hash: "sha256:abc".to_string(),
            export_settings: ExportSettings {
                format: "PNG".to_string(),
                scale: 1.0,
                svg_suffix: false,
            },
        };
        let value = serde_json::to_value(&asset).expect("serializes");
        assert!(value.get("asset_id").is_some());
    }
}
