//! Normalization.
//!
//! The plugin sends what the Figma API exposes. The normalized model is what
//! an agent consumes. This module performs that translation and, crucially,
//! records what could not be translated instead of dropping it.
//!
//! Rules:
//! - an unknown node type survives normalization as an unknown type;
//! - a field the platform did not expose becomes an unavailable field with a
//!   reason, never an empty value;
//! - a mixed style text node becomes a list of runs;
//! - a reference that could not be resolved stays an unresolved reference.

use figtether_model::normalized::{
    Color, ComponentRef, Constraints, Effect, EffectKind, Geometry, GradientStop, Layout,
    NodeSummary, NormalizedNode, Padding, Paint, PaintKind, Rect, SizingMode, TextRun, Transform,
    Typography, VariableRef, VariantProperty,
};
use figtether_protocol::plugin::{
    PluginComponent, PluginDocument, PluginEffect, PluginGeometry, PluginLayout, PluginNode,
    PluginPaint, PluginText, PluginVariable,
};

/// The platform did not expose the property.
pub const REASON_CAPABILITY_UNAVAILABLE: &str = "capability_unavailable";
/// The caller's field projection left the property out.
pub const REASON_PROJECTION_OMITTED: &str = "projection_omitted";
/// The property is meaningless for this node type.
pub const REASON_NOT_APPLICABLE: &str = "not_applicable";
/// The property is representable only visually.
pub const REASON_VISUAL_GAP: &str = "visual_gap";
/// The entity is outside the grant.
pub const REASON_NOT_IN_GRANT: &str = "not_in_grant";

/// Node types that can carry an auto layout.
const LAYOUT_CAPABLE_TYPES: [&str; 4] = ["FRAME", "COMPONENT", "COMPONENT_SET", "INSTANCE"];

/// Node types that are instances of a component.
const INSTANCE_TYPES: [&str; 2] = ["INSTANCE", "COMPONENT_INSTANCE"];

/// Normalize a plugin document into the model agents consume.
///
/// Returns the normalized nodes in document order plus the list of fields
/// that were unavailable anywhere in the document.
pub fn normalize_document(document: &PluginDocument) -> (Vec<NormalizedNode>, Vec<String>) {
    let mut nodes = Vec::with_capacity(document.nodes.len());
    let mut unavailable: Vec<String> = Vec::new();

    for node in &document.nodes {
        let normalized = normalize_node(node);
        for field in &normalized.unavailable_fields {
            push_unique(&mut unavailable, field.clone());
        }
        nodes.push(normalized);
    }

    (nodes, unavailable)
}

/// Normalize one node of a plugin document.
fn normalize_node(node: &PluginNode) -> NormalizedNode {
    let mut unavailable: Vec<String> = Vec::new();

    let summary = NodeSummary {
        id: node.id.clone(),
        node_type: node.node_type.clone(),
        name: node.name.clone(),
        visible: node.visible,
        locked: node.locked,
        child_count: node.children.len() as u32,
    };

    let geometry = node.geometry.as_ref().map(normalize_geometry);
    if geometry.is_none() {
        push_unique(
            &mut unavailable,
            unavailable_field("geometry", REASON_CAPABILITY_UNAVAILABLE),
        );
    }

    let layout = node.layout.as_ref().map(normalize_layout);
    if layout.is_none() && LAYOUT_CAPABLE_TYPES.contains(&node.node_type.as_str()) {
        push_unique(
            &mut unavailable,
            unavailable_field("layout", REASON_CAPABILITY_UNAVAILABLE),
        );
    }

    let typography = node.text.as_ref().map(normalize_text);
    match node.text.as_ref() {
        None => {
            if node.node_type == "TEXT" {
                push_unique(
                    &mut unavailable,
                    unavailable_field("typography", REASON_CAPABILITY_UNAVAILABLE),
                );
            }
        }
        Some(text) => {
            // The characters are there, but the platform did not expose the
            // styling of them. The text keeps its content and the missing
            // styling is reported.
            if text.runs.is_empty() && !text.text.is_empty() {
                push_unique(
                    &mut unavailable,
                    unavailable_field("typography.runs", REASON_CAPABILITY_UNAVAILABLE),
                );
            }
        }
    }

    let component = node.component.as_ref().map(normalize_component);
    if component.is_none() && INSTANCE_TYPES.contains(&node.node_type.as_str()) {
        push_unique(
            &mut unavailable,
            unavailable_field("components", REASON_CAPABILITY_UNAVAILABLE),
        );
    }

    // The plugin reports the fields it could not read itself. The report is
    // kept verbatim, with a reason appended when the plugin gave none.
    for field in &node.unavailable_fields {
        push_unique(&mut unavailable, normalize_unavailable_field(field));
    }

    NormalizedNode {
        summary,
        geometry,
        layout,
        typography,
        fills: node.fills.iter().map(normalize_paint).collect(),
        strokes: node.strokes.iter().map(normalize_paint).collect(),
        effects: node.effects.iter().map(normalize_effect).collect(),
        component,
        variables: node.variables.iter().map(normalize_variable).collect(),
        styles: node.styles.iter().map(normalize_variable).collect(),
        assets: Vec::new(),
        unavailable_fields: unavailable,
    }
}

/// Normalize the geometry of a node.
///
/// The coordinate space the plugin declared is kept as it is: the model never
/// mixes a local space with a page space.
fn normalize_geometry(geometry: &PluginGeometry) -> Geometry {
    Geometry {
        local_transform: transform_of(&geometry.local_transform, geometry.rotation),
        absolute_transform: transform_of(&geometry.absolute_transform, geometry.rotation),
        bounding_box: rect_of(&geometry.bounding_box),
        constraints: Constraints {
            horizontal: geometry.constraints_horizontal.clone(),
            vertical: geometry.constraints_vertical.clone(),
        },
        coordinate_space: geometry.coordinate_space.clone(),
    }
}

/// Normalize the auto layout of a node.
fn normalize_layout(layout: &PluginLayout) -> Layout {
    Layout {
        auto_layout: layout.auto_layout,
        axis: layout.axis.clone(),
        gap: layout.gap,
        padding: Padding {
            top: layout.padding[0],
            right: layout.padding[1],
            bottom: layout.padding[2],
            left: layout.padding[3],
        },
        primary_axis_align: layout.primary_axis_align.clone(),
        counter_axis_align: layout.counter_axis_align.clone(),
        sizing: sizing_mode(&layout.sizing),
        wraps: layout.wraps,
    }
}

/// Normalize the typography of a text node.
///
/// A text with mixed styles arrives as several runs and keeps them. A text
/// whose styling the platform did not expose keeps its characters and becomes
/// one run, and the missing styling is reported.
fn normalize_text(text: &PluginText) -> Typography {
    let mut runs: Vec<TextRun> = text.runs.iter().map(normalize_run).collect();
    if runs.is_empty() && !text.text.is_empty() {
        runs.push(TextRun {
            start: 0,
            end: text.text.chars().count(),
            font_family: String::new(),
            font_style: String::new(),
            font_weight: 0,
            font_size: 0.0,
            line_height: 0.0,
            letter_spacing: 0.0,
            font_available: false,
        });
    }
    Typography {
        text: text.text.clone(),
        runs,
        text_align: text.text_align.clone(),
        truncated: text.truncated,
    }
}

/// Normalize one styled run of a text node.
fn normalize_run(run: &figtether_protocol::plugin::PluginTextRun) -> TextRun {
    TextRun {
        start: run.start,
        end: run.end,
        font_family: run.font_family.clone(),
        font_style: run.font_style.clone(),
        font_weight: run.font_weight,
        font_size: run.font_size,
        line_height: run.line_height,
        letter_spacing: run.letter_spacing,
        font_available: run.font_available,
    }
}

/// Normalize a fill or a stroke.
fn normalize_paint(paint: &PluginPaint) -> Paint {
    Paint {
        kind: paint_kind(&paint.kind),
        color: paint.color.map(color_of),
        gradient_stops: paint
            .gradient_stops
            .iter()
            .map(|stop| GradientStop {
                position: stop.position,
                color: color_of(stop.color),
            })
            .collect(),
        opacity: paint.opacity,
        visible: paint.visible,
    }
}

/// Normalize an effect.
fn normalize_effect(effect: &PluginEffect) -> Effect {
    Effect {
        kind: effect_kind(&effect.kind),
        visible: effect.visible,
        radius: effect.radius,
        color: effect.color.map(color_of),
    }
}

/// Normalize a component reference of an instance node.
fn normalize_component(component: &PluginComponent) -> ComponentRef {
    ComponentRef {
        instance_id: component.instance_id.clone(),
        main_component_id: component.main_component_id.clone(),
        variant_properties: component
            .variant_properties
            .iter()
            .map(|variant| VariantProperty {
                name: variant.name.clone(),
                value: variant.value.clone(),
            })
            .collect(),
        resolved: component.main_component_id.is_some(),
    }
}

/// Normalize a variable or style reference.
///
/// A reference that did not resolve stays unresolved and keeps the reason it
/// did not resolve.
fn normalize_variable(variable: &PluginVariable) -> VariableRef {
    let unresolved_reason = if variable.resolved {
        None
    } else {
        Some(
            variable
                .unresolved_reason
                .clone()
                .unwrap_or_else(|| REASON_CAPABILITY_UNAVAILABLE.to_string()),
        )
    };
    VariableRef {
        id: variable.id.clone(),
        name: variable.name.clone(),
        collection: variable.collection.clone(),
        mode: variable.mode.clone(),
        value: variable.value.clone(),
        alias_target: variable.alias_target.clone(),
        resolved: variable.resolved,
        unresolved_reason,
    }
}

/// The kind of a paint.
fn paint_kind(kind: &str) -> PaintKind {
    match kind.trim().to_ascii_uppercase().as_str() {
        "SOLID" => PaintKind::Solid,
        "GRADIENT" => PaintKind::Gradient,
        "IMAGE" => PaintKind::Image,
        _ => PaintKind::Unknown,
    }
}

/// The kind of an effect.
fn effect_kind(kind: &str) -> EffectKind {
    match kind.trim().to_ascii_uppercase().as_str() {
        "DROP_SHADOW" => EffectKind::DropShadow,
        "INNER_SHADOW" => EffectKind::InnerShadow,
        "BLUR" => EffectKind::Blur,
        "BACKGROUND_BLUR" => EffectKind::BackgroundBlur,
        _ => EffectKind::Unknown,
    }
}

/// The sizing mode of an auto layout container.
fn sizing_mode(sizing: &str) -> SizingMode {
    match sizing.trim().to_ascii_uppercase().as_str() {
        "HUG" => SizingMode::Hug,
        "FILL" => SizingMode::Fill,
        _ => SizingMode::Fixed,
    }
}

/// A transform of four numbers: translation x, translation y, scale x, scale
/// y. The rotation is reported next to it by the plugin.
fn transform_of(values: &[f64; 4], rotation: f64) -> Transform {
    Transform {
        x: values[0],
        y: values[1],
        rotation,
        scale_x: values[2],
        scale_y: values[3],
    }
}

/// A rectangle of four numbers: x, y, width, height.
fn rect_of(values: &[f64; 4]) -> Rect {
    Rect {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    }
}

/// A colour of four channels.
fn color_of(values: [f64; 4]) -> Color {
    Color {
        r: values[0],
        g: values[1],
        b: values[2],
        a: values[3],
    }
}

/// An unavailable field with its reason.
fn unavailable_field(field: &str, reason: &str) -> String {
    format!("{field}: {reason}")
}

/// Normalize the report of the plugin about a field it could not read.
///
/// The reason vocabulary is closed, so an unknown reason falls back to
/// `capability_unavailable`.
fn normalize_unavailable_field(field: &str) -> String {
    match field.split_once(':') {
        Some((name, reason)) => {
            let name = name.trim();
            let reason = normalize_reason(reason.trim());
            if name.is_empty() {
                unavailable_field("field", reason)
            } else {
                unavailable_field(name, reason)
            }
        }
        None => unavailable_field(field.trim(), REASON_CAPABILITY_UNAVAILABLE),
    }
}

/// Map a free-form reason onto the closed vocabulary.
fn normalize_reason(reason: &str) -> &'static str {
    match reason.trim().to_ascii_lowercase().as_str() {
        REASON_CAPABILITY_UNAVAILABLE => REASON_CAPABILITY_UNAVAILABLE,
        REASON_PROJECTION_OMITTED => REASON_PROJECTION_OMITTED,
        REASON_NOT_APPLICABLE => REASON_NOT_APPLICABLE,
        REASON_VISUAL_GAP => REASON_VISUAL_GAP,
        REASON_NOT_IN_GRANT => REASON_NOT_IN_GRANT,
        _ => REASON_CAPABILITY_UNAVAILABLE,
    }
}

/// Append a value when it is not in the list yet.
fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_protocol::plugin::{PluginGradientStop, PluginTextRun, PluginVariantProperty};

    fn node(node_type: &str) -> PluginNode {
        PluginNode {
            id: "1:2".to_string(),
            parent_id: Some("1:1".to_string()),
            node_type: node_type.to_string(),
            name: "node".to_string(),
            visible: true,
            locked: false,
            children: vec!["1:3".to_string()],
            geometry: None,
            layout: None,
            text: None,
            fills: Vec::new(),
            strokes: Vec::new(),
            effects: Vec::new(),
            component: None,
            variables: Vec::new(),
            styles: Vec::new(),
            unavailable_fields: Vec::new(),
        }
    }

    fn document(nodes: Vec<PluginNode>) -> PluginDocument {
        PluginDocument {
            document_label: "design".to_string(),
            page_name: "page".to_string(),
            nodes,
            capabilities: Default::default(),
        }
    }

    #[test]
    fn a_node_keeps_its_identity() {
        let (nodes, unavailable) = normalize_document(&document(vec![node("FRAME")]));
        assert_eq!(nodes.len(), 1);
        let summary = &nodes[0].summary;
        assert_eq!(summary.id, "1:2");
        assert_eq!(summary.node_type, "FRAME");
        assert_eq!(summary.name, "node");
        assert!(summary.visible);
        assert!(!summary.locked);
        assert_eq!(summary.child_count, 1);
        assert!(unavailable
            .iter()
            .any(|field| field.starts_with("geometry")));
    }

    #[test]
    fn an_unknown_node_type_is_preserved_verbatim() {
        let (nodes, _) = normalize_document(&document(vec![node("WIDGET_OF_THE_FUTURE")]));
        assert_eq!(nodes[0].summary.node_type, "WIDGET_OF_THE_FUTURE");
        assert_eq!(nodes[0].summary.name, "node");
        assert_eq!(nodes[0].summary.child_count, 1);
    }

    #[test]
    fn a_missing_geometry_is_reported_as_unavailable() {
        let (_, unavailable) = normalize_document(&document(vec![node("FRAME")]));
        assert!(unavailable
            .iter()
            .any(|field| field == "geometry: capability_unavailable"));
    }

    #[test]
    fn the_unavailable_fields_are_the_union_of_the_document() {
        let mut first = node("FRAME");
        first.unavailable_fields = vec!["typography.line_height_px".to_string()];
        let mut second = node("TEXT");
        second.unavailable_fields = vec!["typography.line_height_px".to_string()];
        second.unavailable_fields.push("paint.effects".to_string());
        let (_, unavailable) = normalize_document(&document(vec![first, second]));
        assert_eq!(
            unavailable
                .iter()
                .filter(|field| field.starts_with("typography.line_height_px"))
                .count(),
            1,
            "the same field is reported once"
        );
        assert!(unavailable
            .iter()
            .any(|field| field.starts_with("paint.effects")));
    }

    #[test]
    fn an_unknown_reason_falls_back_to_capability_unavailable() {
        let mut node = node("FRAME");
        node.unavailable_fields = vec!["paint.effects: something_else".to_string()];
        let (_, unavailable) = normalize_document(&document(vec![node]));
        assert!(unavailable
            .iter()
            .any(|field| field == "paint.effects: capability_unavailable"));
    }

    #[test]
    fn a_known_reason_is_kept() {
        let mut node = node("FRAME");
        node.unavailable_fields = vec!["paint.effects: visual_gap".to_string()];
        let (_, unavailable) = normalize_document(&document(vec![node]));
        assert!(unavailable
            .iter()
            .any(|field| field == "paint.effects: visual_gap"));
    }

    #[test]
    fn the_geometry_keeps_the_declared_coordinate_space() {
        let mut node = node("FRAME");
        node.geometry = Some(PluginGeometry {
            local_transform: [10.0, 20.0, 1.0, 1.0],
            absolute_transform: [110.0, 220.0, 1.0, 1.0],
            bounding_box: [0.0, 0.0, 320.0, 56.0],
            rotation: 15.0,
            constraints_horizontal: "LEFT".to_string(),
            constraints_vertical: "TOP".to_string(),
            coordinate_space: "page".to_string(),
        });
        let (nodes, _) = normalize_document(&document(vec![node]));
        let geometry = nodes[0].geometry.clone().expect("geometry");
        assert_eq!(geometry.coordinate_space, "page");
        assert_eq!(geometry.local_transform.x, 10.0);
        assert_eq!(geometry.local_transform.y, 20.0);
        assert_eq!(geometry.local_transform.rotation, 15.0);
        assert_eq!(geometry.absolute_transform.x, 110.0);
        assert_eq!(geometry.bounding_box.width, 320.0);
        assert_eq!(geometry.constraints.horizontal, "LEFT");
        assert_eq!(geometry.constraints.vertical, "TOP");
    }

    #[test]
    fn the_layout_is_normalized() {
        let mut node = node("FRAME");
        node.layout = Some(PluginLayout {
            auto_layout: true,
            axis: "HORIZONTAL".to_string(),
            gap: 8.0,
            padding: [16.0, 16.0, 16.0, 16.0],
            primary_axis_align: "MIN".to_string(),
            counter_axis_align: "CENTER".to_string(),
            sizing: "HUG".to_string(),
            wraps: false,
        });
        let (nodes, _) = normalize_document(&document(vec![node]));
        let layout = nodes[0].layout.clone().expect("layout");
        assert!(layout.auto_layout);
        assert_eq!(layout.axis, "HORIZONTAL");
        assert_eq!(layout.gap, 8.0);
        assert_eq!(layout.padding.top, 16.0);
        assert_eq!(layout.padding.left, 16.0);
        assert_eq!(layout.sizing, SizingMode::Hug);
    }

    #[test]
    fn a_mixed_style_text_becomes_runs() {
        let mut node = node("TEXT");
        node.text = Some(PluginText {
            text: "Pay now".to_string(),
            runs: vec![
                PluginTextRun {
                    start: 0,
                    end: 3,
                    font_family: "Inter".to_string(),
                    font_style: "Regular".to_string(),
                    font_weight: 400,
                    font_size: 14.0,
                    line_height: 20.0,
                    letter_spacing: 0.2,
                    font_available: true,
                },
                PluginTextRun {
                    start: 4,
                    end: 7,
                    font_family: "Inter".to_string(),
                    font_style: "Semi Bold".to_string(),
                    font_weight: 600,
                    font_size: 14.0,
                    line_height: 20.0,
                    letter_spacing: 0.2,
                    font_available: true,
                },
            ],
            text_align: "LEFT".to_string(),
            truncated: false,
        });
        let (nodes, _) = normalize_document(&document(vec![node]));
        let typography = nodes[0].typography.clone().expect("typography");
        assert_eq!(typography.text, "Pay now");
        assert_eq!(typography.runs.len(), 2);
        assert_eq!(typography.runs[0].font_style, "Regular");
        assert_eq!(typography.runs[1].font_weight, 600);
        assert_eq!(typography.runs[1].end, 7);
    }

    #[test]
    fn a_text_without_runs_keeps_its_characters_and_reports_the_styling() {
        let mut node = node("TEXT");
        node.text = Some(PluginText {
            text: "Pay now".to_string(),
            runs: Vec::new(),
            text_align: "LEFT".to_string(),
            truncated: false,
        });
        let (nodes, unavailable) = normalize_document(&document(vec![node]));
        let typography = nodes[0].typography.clone().expect("typography");
        assert_eq!(typography.text, "Pay now");
        assert_eq!(typography.runs.len(), 1);
        assert_eq!(typography.runs[0].end, 7);
        assert!(!typography.runs[0].font_available);
        assert!(unavailable
            .iter()
            .any(|field| field.starts_with("typography.runs")));
    }

    #[test]
    fn the_paint_kinds_are_mapped() {
        let mut node = node("FRAME");
        node.fills = vec![
            PluginPaint {
                kind: "SOLID".to_string(),
                color: Some([1.0, 0.0, 0.0, 1.0]),
                gradient_stops: Vec::new(),
                opacity: 1.0,
                visible: true,
            },
            PluginPaint {
                kind: "GRADIENT".to_string(),
                color: None,
                gradient_stops: vec![PluginGradientStop {
                    position: 0.5,
                    color: [0.0, 0.0, 1.0, 1.0],
                }],
                opacity: 1.0,
                visible: true,
            },
            PluginPaint {
                kind: "IMAGE".to_string(),
                color: None,
                gradient_stops: Vec::new(),
                opacity: 0.5,
                visible: false,
            },
            PluginPaint {
                kind: "SOMETHING_ELSE".to_string(),
                color: None,
                gradient_stops: Vec::new(),
                opacity: 1.0,
                visible: true,
            },
        ];
        let (nodes, _) = normalize_document(&document(vec![node]));
        let fills = &nodes[0].fills;
        assert_eq!(fills.len(), 4);
        assert_eq!(fills[0].kind, PaintKind::Solid);
        assert_eq!(fills[0].color.expect("colour").r, 1.0);
        assert_eq!(fills[1].kind, PaintKind::Gradient);
        assert_eq!(fills[1].gradient_stops.len(), 1);
        assert_eq!(fills[1].gradient_stops[0].position, 0.5);
        assert_eq!(fills[2].kind, PaintKind::Image);
        assert_eq!(fills[3].kind, PaintKind::Unknown);
    }

    #[test]
    fn the_effect_kinds_are_mapped() {
        let mut node = node("FRAME");
        node.effects = vec![
            PluginEffect {
                kind: "DROP_SHADOW".to_string(),
                visible: true,
                radius: 4.0,
                color: Some([0.0, 0.0, 0.0, 0.2]),
            },
            PluginEffect {
                kind: "INNER_SHADOW".to_string(),
                visible: true,
                radius: 2.0,
                color: None,
            },
            PluginEffect {
                kind: "BLUR".to_string(),
                visible: true,
                radius: 8.0,
                color: None,
            },
            PluginEffect {
                kind: "BACKGROUND_BLUR".to_string(),
                visible: true,
                radius: 12.0,
                color: None,
            },
            PluginEffect {
                kind: "LAYER_BLUR".to_string(),
                visible: true,
                radius: 1.0,
                color: None,
            },
        ];
        let (nodes, _) = normalize_document(&document(vec![node]));
        let effects = &nodes[0].effects;
        assert_eq!(effects.len(), 5);
        assert_eq!(effects[0].kind, EffectKind::DropShadow);
        assert_eq!(effects[0].color.expect("colour").a, 0.2);
        assert_eq!(effects[1].kind, EffectKind::InnerShadow);
        assert_eq!(effects[2].kind, EffectKind::Blur);
        assert_eq!(effects[3].kind, EffectKind::BackgroundBlur);
        assert_eq!(effects[4].kind, EffectKind::Unknown);
    }

    #[test]
    fn an_unresolved_variable_stays_unresolved_with_its_reason() {
        let mut node = node("FRAME");
        node.variables = vec![PluginVariable {
            id: "VariableID:1:2".to_string(),
            name: "accent".to_string(),
            collection: Some("colors".to_string()),
            mode: Some("light".to_string()),
            value: None,
            alias_target: None,
            resolved: false,
            unresolved_reason: Some("not_in_grant".to_string()),
        }];
        node.styles = vec![PluginVariable {
            id: "S:abc".to_string(),
            name: "fill".to_string(),
            collection: None,
            mode: None,
            value: None,
            alias_target: None,
            resolved: false,
            unresolved_reason: None,
        }];
        let (nodes, _) = normalize_document(&document(vec![node]));
        let variable = &nodes[0].variables[0];
        assert!(!variable.resolved);
        assert_eq!(variable.unresolved_reason.as_deref(), Some("not_in_grant"));
        assert!(variable.value.is_none());
        let style = &nodes[0].styles[0];
        assert!(!style.resolved);
        assert_eq!(
            style.unresolved_reason.as_deref(),
            Some(REASON_CAPABILITY_UNAVAILABLE),
            "a missing reason falls back to the closed vocabulary"
        );
    }

    #[test]
    fn a_component_reference_is_normalized() {
        let mut node = node("INSTANCE");
        node.component = Some(PluginComponent {
            instance_id: "2:2".to_string(),
            main_component_id: Some("2:1".to_string()),
            variant_properties: vec![PluginVariantProperty {
                name: "state".to_string(),
                value: "hover".to_string(),
            }],
        });
        let (nodes, _) = normalize_document(&document(vec![node]));
        let component = nodes[0].component.clone().expect("component");
        assert_eq!(component.instance_id, "2:2");
        assert_eq!(component.main_component_id.as_deref(), Some("2:1"));
        assert!(component.resolved);
        assert_eq!(component.variant_properties.len(), 1);
        assert_eq!(component.variant_properties[0].value, "hover");
    }

    #[test]
    fn an_instance_without_a_component_is_reported() {
        let (_, unavailable) = normalize_document(&document(vec![node("INSTANCE")]));
        assert!(unavailable
            .iter()
            .any(|field| field.starts_with("components")));
    }

    #[test]
    fn the_nodes_keep_the_document_order() {
        let mut first = node("FRAME");
        first.id = "1:1".to_string();
        let mut second = node("TEXT");
        second.id = "1:2".to_string();
        let (nodes, _) = normalize_document(&document(vec![first, second]));
        assert_eq!(nodes[0].summary.id, "1:1");
        assert_eq!(nodes[1].summary.id, "1:2");
    }

    #[test]
    fn an_empty_document_normalizes_to_nothing() {
        let (nodes, unavailable) = normalize_document(&document(Vec::new()));
        assert!(nodes.is_empty());
        assert!(unavailable.is_empty());
    }
}
