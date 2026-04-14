use std::collections::BTreeMap;

use egui::{RichText, Ui};

use crate::{
    extraction::models::{ExtractedDrawing, SceneEntity, StyleSource},
    render::painter::RenderStats,
};

pub const WGPU_RECOMMENDATION_THRESHOLD: usize = 120_000;

#[derive(Debug, Default, Clone)]
pub struct VisibilityFilters {
    pub layer_visibility: BTreeMap<String, bool>,
    pub block_visibility: BTreeMap<String, bool>,
    pub layer_search: String,
    pub block_search: String,
}

impl VisibilityFilters {
    pub fn sync_from_drawing(&mut self, drawing: &ExtractedDrawing) {
        for layer in drawing.layers.values() {
            self.layer_visibility
                .entry(layer.name.clone())
                .or_insert(layer.visible_by_default);
        }

        for block in drawing.blocks.values() {
            self.block_visibility
                .entry(block.name.clone())
                .or_insert(true);
        }
    }

    pub fn is_entity_visible(&self, entity: &SceneEntity) -> bool {
        let layer_visible = self
            .layer_visibility
            .get(&entity.layer_name)
            .copied()
            .unwrap_or(true);
        if !layer_visible {
            return false;
        }
        if let Some(block_name) = &entity.block_name {
            self.block_visibility.get(block_name).copied().unwrap_or(true)
        } else {
            true
        }
    }

    pub fn set_all_layers(&mut self, visible: bool) {
        for value in self.layer_visibility.values_mut() {
            *value = visible;
        }
    }

    pub fn set_all_blocks(&mut self, visible: bool) {
        for value in self.block_visibility.values_mut() {
            *value = visible;
        }
    }
}

pub struct SidebarActions {
    pub request_fit_view: bool,
}

pub fn show_sidebar(
    ui: &mut Ui,
    drawing: &ExtractedDrawing,
    filters: &mut VisibilityFilters,
    selected: Option<&SceneEntity>,
    render_stats: &RenderStats,
    selected_transform_chain: &[String],
    selected_style_source: Option<StyleSource>,
) -> SidebarActions {
    let mut request_fit_view = false;
    ui.heading("Drawing Controls");
    ui.label(format!("File: {}", drawing.source_path.display()));
    ui.separator();

    if ui.button("Fit To View").clicked() {
        request_fit_view = true;
    }

    ui.label(
        RichText::new(format!(
            "Entities: {} (renderable: {})",
            drawing.stats.total_entities, drawing.stats.renderable_entities
        ))
        .small(),
    );
    ui.label(RichText::new(format!("Load: {} ms", drawing.stats.load_duration_ms)).small());
    ui.label(
        RichText::new(format!(
            "Drawn: {} | Culled: {} | Traversed: {}",
            render_stats.drawn_entities, render_stats.culled_entities, render_stats.traversed_entities
        ))
        .small(),
    );

    if drawing.stats.total_entities > WGPU_RECOMMENDATION_THRESHOLD {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "Large drawing detected. Consider WGPU renderer milestone for smoother interaction.",
        );
    }

    ui.separator();
    ui.collapsing("Layers", |ui| {
        ui.horizontal(|ui| {
            if ui.button("Show all").clicked() {
                filters.set_all_layers(true);
            }
            if ui.button("Hide all").clicked() {
                filters.set_all_layers(false);
            }
        });
        ui.text_edit_singleline(&mut filters.layer_search);
        let needle = filters.layer_search.to_ascii_lowercase();
        for layer in drawing.layers.values() {
            if !needle.is_empty() && !layer.name.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            let visibility = filters
                .layer_visibility
                .entry(layer.name.clone())
                .or_insert(layer.visible_by_default);
            ui.checkbox(visibility, format!("{} ({})", layer.name, layer.entity_count));
        }
    });

    ui.collapsing("Blocks", |ui| {
        ui.horizontal(|ui| {
            if ui.button("Show all").clicked() {
                filters.set_all_blocks(true);
            }
            if ui.button("Hide all").clicked() {
                filters.set_all_blocks(false);
            }
        });
        ui.text_edit_singleline(&mut filters.block_search);
        let needle = filters.block_search.to_ascii_lowercase();
        for block in drawing.blocks.values() {
            if !needle.is_empty() && !block.name.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            let visibility = filters
                .block_visibility
                .entry(block.name.clone())
                .or_insert(true);
            ui.checkbox(visibility, format!("{} ({})", block.name, block.entity_count));
        }
    });

    ui.separator();
    ui.heading("Selection");
    if let Some(entity) = selected {
        ui.monospace(format!("Entity #{:X}", entity.handle));
        ui.label(format!("Type: {}", entity.entity_type));
        ui.label(format!("Layer: {}", entity.layer_name));
        ui.label(format!(
            "Block: {}",
            entity.block_name.as_deref().unwrap_or("<model-space>")
        ));
        if let Some(source) = selected_style_source {
            ui.label(format!("Style source: {:?}", source));
        }
        if !selected_transform_chain.is_empty() {
            ui.label("Transform chain:");
            for item in selected_transform_chain {
                ui.monospace(item);
            }
        }
    } else {
        ui.label("No entity selected.");
    }

    SidebarActions { request_fit_view }
}
