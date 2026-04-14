use std::path::Path;
use std::collections::BTreeSet;

use eframe::egui;
use rfd::FileDialog;

use crate::{
    extraction::{self, ExtractedDrawing},
    render::painter::{self, ViewportState},
    ui::panels::{show_sidebar, VisibilityFilters},
};

#[derive(Default)]
pub struct CadViewerApp {
    drawing: Option<ExtractedDrawing>,
    filters: VisibilityFilters,
    viewport: ViewportState,
    selected_entity: Option<usize>,
    hovered_world: Option<(f64, f64)>,
    status: String,
    last_render_stats: painter::RenderStats,
    selected_transform_chain: Vec<String>,
    selected_style_source: Option<crate::extraction::models::StyleSource>,
    request_fit_view: bool,
}

impl CadViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();

        let font_paths = [
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
        ];

        for path in font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "cjk_fallback".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(font_data)),
                );
                
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                break;
            }
        }

        cc.egui_ctx.set_fonts(fonts);

        Self {
            status: "Open a DXF or DWG file to start.".to_owned(),
            request_fit_view: false,
            ..Self::default()
        }
    }

    fn open_file_dialog(&mut self) {
        let file = FileDialog::new()
            .add_filter("CAD Files", &["dxf", "dwg"])
            .pick_file();

        if let Some(path) = file {
            self.load_path(path.as_path());
        }
    }

    fn load_path(&mut self, path: &Path) {
        match extraction::extract_file(path) {
            Ok(drawing) => {
                self.filters = VisibilityFilters::default();
                self.filters.sync_from_drawing(&drawing);
                self.viewport = ViewportState::default();
                self.selected_entity = None;
                self.last_render_stats = painter::RenderStats::default();
                self.selected_transform_chain.clear();
                self.selected_style_source = None;
                self.request_fit_view = true;
                self.status = format!(
                    "Loaded {} entities from {}",
                    drawing.stats.total_entities,
                    drawing.source_path.display()
                );
                self.drawing = Some(drawing);
            }
            Err(error) => {
                self.status = format!("Load failed: {error}");
            }
        }
    }
}

impl eframe::App for CadViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx();
        egui::TopBottomPanel::top("top_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.open_file_dialog();
                }
                if ui.button("Fit View").clicked() {
                    self.request_fit_view = true;
                }
                if ui.button("Clear Selection").clicked() {
                    self.selected_entity = None;
                }
            });
            ui.separator();
            ui.label(self.status.as_str());
            if let Some((x, y)) = self.hovered_world {
                ui.monospace(format!("Cursor (world): ({x:.3}, {y:.3})"));
            }
        });

        let Some(drawing) = self.drawing.as_ref() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("CAD Extraction Viewer");
                    ui.label("Use Open to load a DWG/DXF file.");
                });
            });
            return;
        };

        self.filters.sync_from_drawing(drawing);

        egui::SidePanel::left("left_filters")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                let selected_entity = self
                    .selected_entity
                    .and_then(|id| drawing.entities.get(id));
                let actions = show_sidebar(
                    ui,
                    drawing,
                    &mut self.filters,
                    selected_entity,
                    &self.last_render_stats,
                    &self.selected_transform_chain,
                    self.selected_style_source,
                );
                if actions.request_fit_view {
                    self.request_fit_view = true;
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.request_fit_view {
                self.viewport = ViewportState::default();
                self.viewport
                    .fit_to_bounds(drawing.bounds, ui.available_rect_before_wrap());
                self.request_fit_view = false;
            }

            let mut visible_entity_ids = BTreeSet::new();
            for entity in &drawing.entities {
                if self.filters.is_entity_visible(entity) {
                    visible_entity_ids.insert(entity.id);
                }
            }

            if self.viewport.zoom <= 0.011 {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "Zoom is very low; increase zoom for precise selection.",
                );
            }

            let render_output = painter::draw_scene(
                ui,
                &mut self.viewport,
                drawing,
                &visible_entity_ids,
                self.selected_entity,
            );
            self.last_render_stats = render_output.stats.clone();
            self.selected_transform_chain = render_output.selected_transform_chain.clone();
            self.selected_style_source = render_output.selected_style_source;
            self.hovered_world = render_output
                .hovered_world
                .map(|point| (point.x, point.y));

            if render_output.response.clicked() {
                if let Some(position) = render_output.response.interact_pointer_pos() {
                    self.selected_entity = painter::pick_entity(
                        drawing,
                        &self.viewport,
                        position,
                        &visible_entity_ids,
                        8.0,
                    );
                }
            }

            if !visible_entity_ids.is_empty() {
                ui.label(format!(
                    "Visible entities after filters: {}",
                    visible_entity_ids.len()
                ));
            } else {
                ui.label("No entities are visible with current filters.");
            }
        });
    }
}
