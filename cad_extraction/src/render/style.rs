use egui::Color32;

use crate::extraction::models::{
    CadColorSpec, CadLineWeightSpec, ExtractedDrawing, SceneEntity, StyleSource,
};

#[derive(Debug, Clone, Copy)]
pub struct ResolvedStyle {
    pub color: Color32,
    pub line_width: f32,
    pub color_source: StyleSource,
}

pub fn resolve_style(entity: &SceneEntity, drawing: &ExtractedDrawing) -> ResolvedStyle {
    let (color, color_source) = resolve_color(entity, drawing);
    let line_width = resolve_line_weight(entity, drawing);
    ResolvedStyle {
        color,
        line_width,
        color_source,
    }
}

fn resolve_color(entity: &SceneEntity, drawing: &ExtractedDrawing) -> (Color32, StyleSource) {
    match entity.style.color {
        CadColorSpec::Rgb(r, g, b) => (Color32::from_rgb(r, g, b), StyleSource::TrueColor),
        CadColorSpec::Index(index) => (aci_to_color(index), StyleSource::Aci),
        CadColorSpec::ByLayer | CadColorSpec::ByBlock => {
            if let Some(layer) = drawing.layers.get(&entity.layer_name) {
                match layer.color {
                    CadColorSpec::Rgb(r, g, b) => {
                        (Color32::from_rgb(r, g, b), StyleSource::Layer)
                    }
                    CadColorSpec::Index(index) => (aci_to_color(index), StyleSource::Layer),
                    _ => (Color32::LIGHT_BLUE, StyleSource::Fallback),
                }
            } else {
                (Color32::LIGHT_BLUE, StyleSource::Fallback)
            }
        }
    }
}

fn resolve_line_weight(entity: &SceneEntity, drawing: &ExtractedDrawing) -> f32 {
    let raw_mm = match entity.style.line_weight {
        CadLineWeightSpec::Value(value) => value as f32 / 100.0,
        CadLineWeightSpec::ByLayer => drawing
            .layers
            .get(&entity.layer_name)
            .and_then(|layer| match layer.line_weight {
                CadLineWeightSpec::Value(value) => Some(value as f32 / 100.0),
                _ => None,
            })
            .unwrap_or(0.25),
        CadLineWeightSpec::ByBlock | CadLineWeightSpec::Default => 0.25,
    };

    (raw_mm * 3.0).clamp(0.4, 3.0)
}

fn aci_to_color(index: u8) -> Color32 {
    match index {
        1 => Color32::from_rgb(255, 0, 0),
        2 => Color32::from_rgb(255, 255, 0),
        3 => Color32::from_rgb(0, 255, 0),
        4 => Color32::from_rgb(0, 255, 255),
        5 => Color32::from_rgb(0, 0, 255),
        6 => Color32::from_rgb(255, 0, 255),
        7 => Color32::from_rgb(255, 255, 255),
        8 => Color32::from_rgb(128, 128, 128),
        9 => Color32::from_rgb(192, 192, 192),
        _ => Color32::from_rgb(200, 200, 200),
    }
}
