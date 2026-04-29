use std::{collections::BTreeSet, path::Path};

use eframe::egui;
use rfd::FileDialog;

use crate::{
    extraction::{self, ExtractedDrawing},
    render::painter::{self, ViewportState},
    ui::{
        commands::{CommandAction, PanelToggle, parse_command},
        panels::{VisibilityFilters, show_properties_panel, show_sidebar},
    },
};

pub struct CadViewerApp {
    drawing: Option<ExtractedDrawing>,
    active_layout_index: usize,
    filters: VisibilityFilters,
    viewport: ViewportState,
    selected_entity: Option<usize>,
    hovered_world: Option<(f64, f64)>,
    status: String,
    last_render_stats: painter::RenderStats,
    selected_transform_chain: Vec<String>,
    selected_style_source: Option<crate::extraction::models::StyleSource>,
    request_fit_view: bool,
    show_sidebar: bool,
    show_properties_panel: bool,
    show_command_line: bool,
    command_input: String,
}

impl Default for CadViewerApp {
    fn default() -> Self {
        Self {
            drawing: None,
            active_layout_index: 0,
            filters: VisibilityFilters::default(),
            viewport: ViewportState::default(),
            selected_entity: None,
            hovered_world: None,
            status: "Open a DXF or DWG file to start.".to_owned(),
            last_render_stats: painter::RenderStats::default(),
            selected_transform_chain: Vec::new(),
            selected_style_source: None,
            request_fit_view: false,
            show_sidebar: true,
            show_properties_panel: true,
            show_command_line: true,
            command_input: String::new(),
        }
    }
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
        Self::default()
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
                let active_layout_index = drawing.layouts.iter().position(|layout| layout.is_model).unwrap_or(0);
                self.filters = VisibilityFilters::default();
                self.filters.sync_from_drawing(&drawing);
                self.active_layout_index = active_layout_index;
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

    fn visible_entity_ids(&self, drawing: &ExtractedDrawing) -> BTreeSet<usize> {
        drawing
            .entities
            .iter()
            .filter(|entity| self.filters.is_entity_visible(entity))
            .map(|entity| entity.id)
            .collect()
    }

    fn active_root_block_name<'a>(&self, drawing: &'a ExtractedDrawing) -> Option<&'a str> {
        drawing
            .layouts
            .get(self.active_layout_index)
            .map(|layout| layout.root_block_name.as_str())
    }

    fn select_layout(&mut self, index: usize) {
        if self.active_layout_index != index {
            self.active_layout_index = index;
            self.selected_entity = None;
            self.selected_transform_chain.clear();
            self.selected_style_source = None;
            self.request_fit_view = true;
        }
    }

    fn apply_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::Open => self.open_file_dialog(),
            CommandAction::FitView => {
                self.request_fit_view = true;
                self.status = "Queued fit-to-view.".to_owned();
            }
            CommandAction::ClearSelection => {
                self.selected_entity = None;
                self.selected_transform_chain.clear();
                self.selected_style_source = None;
                self.status = "Selection cleared.".to_owned();
            }
            CommandAction::SetAllLayers(visible) => {
                self.filters.set_all_layers(visible);
                self.status = if visible {
                    "All layers are now visible.".to_owned()
                } else {
                    "All layers are now hidden.".to_owned()
                };
            }
            CommandAction::SetAllBlocks(visible) => {
                self.filters.set_all_blocks(visible);
                self.status = if visible {
                    "All blocks are now visible.".to_owned()
                } else {
                    "All blocks are now hidden.".to_owned()
                };
            }
            CommandAction::TogglePanel(panel) => {
                let (label, state) = match panel {
                    PanelToggle::Sidebar => ("Sidebar", &mut self.show_sidebar),
                    PanelToggle::Properties => {
                        ("Properties panel", &mut self.show_properties_panel)
                    }
                    PanelToggle::CommandLine => ("Command line", &mut self.show_command_line),
                };
                *state = !*state;
                self.status = format!("{label} {}", if *state { "shown" } else { "hidden" });
            }
        }
    }

    fn execute_command_line(&mut self) {
        let input = self.command_input.trim().to_owned();
        if input.is_empty() {
            self.status = "Enter a command first.".to_owned();
            return;
        }

        match parse_command(&input) {
            Ok(action) => {
                self.apply_command(action);
                self.command_input.clear();
            }
            Err(error) => {
                self.status = error;
            }
        }
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open").clicked() {
                    self.open_file_dialog();
                }
                if ui.button("Fit View").clicked() {
                    self.request_fit_view = true;
                }
                if ui.button("Clear Selection").clicked() {
                    self.selected_entity = None;
                }
                ui.separator();
                ui.toggle_value(&mut self.show_sidebar, "Explorer");
                ui.toggle_value(&mut self.show_properties_panel, "Properties");
                ui.toggle_value(&mut self.show_command_line, "Command");
                ui.separator();

                let title = self
                    .drawing
                    .as_ref()
                    .and_then(|drawing| drawing.source_path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("No drawing loaded");
                ui.strong(title);
            });
            ui.small(self.status.as_str());
        });
    }

    fn draw_layout_tabs(&mut self, ctx: &egui::Context) {
        let Some(drawing) = self.drawing.as_ref() else {
            return;
        };
        if drawing.layouts.is_empty() {
            return;
        }

        let tabs: Vec<(String, String)> = drawing
            .layouts
            .iter()
            .map(|layout| (layout.name.clone(), layout.root_block_name.clone()))
            .collect();

        egui::TopBottomPanel::top("layout_tabs")
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, (name, root_block_name)) in tabs.iter().enumerate() {
                            let selected = self.active_layout_index == index;
                            let response = ui.selectable_label(selected, name);
                            if response.clicked() {
                                self.select_layout(index);
                                self.status =
                                    format!("Switched to layout '{name}' ({root_block_name}).");
                            }
                        }
                    });
                });
            });
    }

    fn draw_status_bar(
        &mut self,
        ctx: &egui::Context,
        visible_entity_count: usize,
        selected_handle: Option<u64>,
    ) {
        let panel_height = if self.show_command_line { 70.0 } else { 30.0 };
        egui::TopBottomPanel::bottom("status_bar")
            .resizable(false)
            .default_height(panel_height)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Zoom {:.2}x", self.viewport.zoom));
                    ui.separator();
                    ui.label(format!("Visible {visible_entity_count}"));
                    if let Some(handle) = selected_handle {
                        ui.separator();
                        ui.monospace(format!("Selected #{handle:X}"));
                    }
                    if let Some((x, y)) = self.hovered_world {
                        ui.separator();
                        ui.monospace(format!("Cursor ({x:.3}, {y:.3})"));
                    }
                });

                if self.show_command_line {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Command");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.command_input)
                                .desired_width(f32::INFINITY)
                                .hint_text(
                                    "open | fit | clear | layers on/off | blocks on/off | toggle properties",
                                ),
                        );
                        let should_run = ui.button("Run").clicked()
                            || (response.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                        if should_run {
                            self.execute_command_line();
                        }
                    });
                }
            });
    }
}

impl eframe::App for CadViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx();
        self.draw_top_bar(ctx);
        self.draw_layout_tabs(ctx);

        if self.drawing.is_none() {
            self.draw_status_bar(ctx, 0, None);
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.heading("CAD Extraction Viewer");
                    ui.label("Rust desktop viewer inspired by cad-viewer.");
                    ui.label("Use Open to load a DWG/DXF file.");
                    ui.small("Command line examples: fit, layers off, blocks on");
                });
            });
            return;
        }

        let (visible_entity_ids, visible_entity_count, selected_handle) = {
            let drawing = self.drawing.as_ref().expect("drawing checked above");
            self.filters.sync_from_drawing(drawing);
            let visible_entity_ids = self.visible_entity_ids(drawing);
            let visible_entity_count = visible_entity_ids.len();
            let selected_handle = self
                .selected_entity
                .and_then(|id| drawing.entities.get(id))
                .map(|entity| entity.handle);
            (visible_entity_ids, visible_entity_count, selected_handle)
        };

        self.draw_status_bar(ctx, visible_entity_count, selected_handle);

        let drawing = self.drawing.as_ref().expect("drawing checked above");
        let active_root = self.active_root_block_name(drawing);
        let active_layout_name = drawing
            .layouts
            .get(self.active_layout_index)
            .map(|layout| layout.name.as_str())
            .unwrap_or("Model");
        let selected_entity = self.selected_entity.and_then(|id| drawing.entities.get(id));
        let mut sidebar_requested_fit = false;

        if self.show_sidebar {
            egui::SidePanel::left("left_filters")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    let actions =
                        show_sidebar(ui, drawing, &mut self.filters, &self.last_render_stats);
                    sidebar_requested_fit = actions.request_fit_view;
                });
        }

        if self.show_properties_panel {
            egui::SidePanel::right("right_properties")
                .resizable(true)
                .default_width(280.0)
                .show(ctx, |ui| {
                    show_properties_panel(
                        ui,
                        drawing,
                        active_layout_name,
                        visible_entity_count,
                        selected_entity,
                        &self.selected_transform_chain,
                        self.selected_style_source,
                        &self.last_render_stats,
                    );
                });
        }

        if sidebar_requested_fit {
            self.request_fit_view = true;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.request_fit_view {
                self.viewport = ViewportState::default();
                let render_bounds =
                    painter::compute_visible_bounds(drawing, &visible_entity_ids, active_root);
                self.viewport
                    .fit_to_bounds(render_bounds.or(drawing.bounds), ui.available_rect_before_wrap());
                self.request_fit_view = false;
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
                active_root,
            );
            self.last_render_stats = render_output.stats.clone();
            self.selected_transform_chain = render_output.selected_transform_chain.clone();
            self.selected_style_source = render_output.selected_style_source;
            self.hovered_world = render_output.hovered_world.map(|point| (point.x, point.y));

            if render_output.response.clicked()
                && let Some(position) = render_output.response.interact_pointer_pos()
            {
                self.selected_entity = painter::pick_entity(
                    drawing,
                    &self.viewport,
                    position,
                    &visible_entity_ids,
                    8.0,
                    active_root,
                );
            }

            if visible_entity_count == 0 {
                ui.label("No entities are visible with current filters.");
            }
        });
    }
}
