use std::collections::BTreeSet;

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::extraction::models::{
    Bounds2D, ExtractedDrawing, HatchBoundaryEdgeGeometry, HatchBoundaryPathGeometry, Point2,
    Polyline2DGeometry, SceneEntity, SceneGeometry, SplineGeometry,
};

use super::{
    style::{ResolvedStyle, resolve_style},
    transform::Affine2,
};

const MAX_RECURSION_DEPTH: usize = 30;
const MIN_CURVE_SEGMENTS: usize = 16;
const MAX_CURVE_SEGMENTS: usize = 720;
const CURVE_TARGET_SEGMENT_PIXELS: f64 = 6.0;
const BOUNDS_CURVE_STEP_RADIANS: f64 = std::f64::consts::PI / 64.0;

#[derive(Debug, Clone)]
pub struct ViewportState {
    pub zoom: f32,
    pub pan: Vec2,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl ViewportState {
    pub fn fit_to_bounds(&mut self, bounds: Option<Bounds2D>, rect: Rect) {
        let Some(bounds) = bounds else {
            return;
        };
        let world_width = bounds.width().max(1.0) as f32;
        let world_height = bounds.height().max(1.0) as f32;
        let zoom_x = rect.width() / world_width;
        let zoom_y = rect.height() / world_height;
        self.zoom = zoom_x.min(zoom_y) * 0.9;

        let center = Point2::new(
            (bounds.min.x + bounds.max.x) * 0.5,
            (bounds.min.y + bounds.max.y) * 0.5,
        );
        let screen_center = rect.center();
        self.pan = screen_center.to_vec2() - self.world_to_screen_raw(center);
    }

    pub fn world_to_screen(&self, world: Point2) -> Pos2 {
        let raw = self.world_to_screen_raw(world);
        Pos2::new(raw.x, raw.y)
    }

    pub fn screen_to_world(&self, screen: Pos2) -> Point2 {
        Point2::new(
            ((screen.x - self.pan.x) / self.zoom) as f64,
            (-(screen.y - self.pan.y) / self.zoom) as f64,
        )
    }

    fn world_to_screen_raw(&self, world: Point2) -> Vec2 {
        Vec2::new(
            (world.x as f32) * self.zoom + self.pan.x,
            (-(world.y as f32)) * self.zoom + self.pan.y,
        )
    }
}

pub struct RenderOutput {
    pub response: Response,
    pub hovered_world: Option<Point2>,
    pub stats: RenderStats,
    pub selected_transform_chain: Vec<String>,
    pub selected_style_source: Option<crate::extraction::models::StyleSource>,
}

#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    pub traversed_entities: usize,
    pub drawn_entities: usize,
    pub culled_entities: usize,
}

pub fn compute_visible_bounds(
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    active_root: Option<&str>,
) -> Option<Bounds2D> {
    let mut bounds = None;
    let root_ids = build_root_draw_queue(drawing, visible_entity_ids, active_root);
    for entity_id in root_ids {
        collect_entity_bounds_recursive(
            drawing,
            visible_entity_ids,
            entity_id,
            Affine2::IDENTITY,
            0,
            &mut Vec::new(),
            &mut bounds,
        );
    }
    bounds
}

pub fn draw_scene(
    ui: &mut Ui,
    viewport: &mut ViewportState,
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    selected_entity: Option<usize>,
    active_root: Option<&str>,
) -> RenderOutput {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());

    if response.dragged() {
        viewport.pan += response.drag_delta();
    }

    if response.hovered() {
        let hover_pos = response.hover_pos();
        ui.input(|input| {
            if input.smooth_scroll_delta.y != 0.0 {
                let zoom_delta = (input.smooth_scroll_delta.y / 250.0).exp();
                let new_zoom = (viewport.zoom * zoom_delta).clamp(0.01, 5_000.0);
                
                if let Some(pointer_pos) = hover_pos {
                    let world_pos = viewport.screen_to_world(pointer_pos);
                    viewport.zoom = new_zoom;
                    viewport.pan.x = pointer_pos.x - (world_pos.x as f32) * viewport.zoom;
                    viewport.pan.y = pointer_pos.y + (world_pos.y as f32) * viewport.zoom;
                } else {
                    viewport.zoom = new_zoom;
                }
            }
        });
    }

    let clip_rect = response.rect;
    painter.rect_stroke(
        clip_rect,
        0.0,
        Stroke::new(1.0, Color32::DARK_GRAY),
        egui::StrokeKind::Outside,
    );

    let mut stats = RenderStats::default();
    let mut selected_transform_chain = Vec::new();
    let mut selected_style_source = None;
    let root_ids = build_root_draw_queue(drawing, visible_entity_ids, active_root);
    for entity_id in root_ids {
        draw_entity_recursive(
            &painter,
            viewport,
            drawing,
            visible_entity_ids,
            entity_id,
            Affine2::IDENTITY,
            0,
            &mut Vec::new(),
            selected_entity,
            &mut selected_transform_chain,
            &mut selected_style_source,
            clip_rect,
            &mut stats,
        );
    }

    let hovered_world = response
        .hover_pos()
        .map(|position| viewport.screen_to_world(position));

    RenderOutput {
        response,
        hovered_world,
        stats,
        selected_transform_chain,
        selected_style_source,
    }
}

pub fn pick_entity(
    drawing: &ExtractedDrawing,
    viewport: &ViewportState,
    click_pos: Pos2,
    visible_entity_ids: &BTreeSet<usize>,
    max_pixels: f32,
    active_root: Option<&str>,
) -> Option<usize> {
    let mut closest: Option<(usize, f32)> = None;
    let roots = build_root_draw_queue(drawing, visible_entity_ids, active_root);
    for entity_id in roots {
        pick_entity_recursive(
            drawing,
            viewport,
            click_pos,
            visible_entity_ids,
            entity_id,
            Affine2::IDENTITY,
            0,
            &mut closest,
        );
    }
    closest
        .filter(|(_, distance)| *distance <= max_pixels)
        .map(|(id, _)| id)
}

fn build_root_draw_queue(
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    active_root: Option<&str>,
) -> Vec<usize> {
    let mut roots: Vec<usize> = drawing
        .entities
        .iter()
        .filter(|entity| {
            visible_entity_ids.contains(&entity.id)
                && is_root_entity(entity.block_name.as_deref(), active_root)
        })
        .map(|entity| entity.id)
        .collect();

    roots.sort_by_key(|id| {
        let is_hatch = matches!(drawing.entities[*id].geometry, SceneGeometry::Hatch { .. });
        (is_hatch as u8, *id)
    });
    roots
}

fn is_model_space_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("*Model_Space")
}

fn is_root_entity(block_name: Option<&str>, active_root: Option<&str>) -> bool {
    match active_root {
        Some(root) if is_model_space_name(root) => {
            block_name.is_none_or(is_model_space_name)
        }
        Some(root) => block_name.is_some_and(|name| name.eq_ignore_ascii_case(root)),
        None => block_name.is_none_or(is_model_space_name),
    }
}

fn collect_entity_bounds_recursive(
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    entity_id: usize,
    transform: Affine2,
    depth: usize,
    block_stack: &mut Vec<String>,
    bounds: &mut Option<Bounds2D>,
) {
    if depth > MAX_RECURSION_DEPTH || !visible_entity_ids.contains(&entity_id) {
        return;
    }

    let entity = &drawing.entities[entity_id];
    if let Some((min, max)) = transformed_bounds(entity, transform) {
        include_bounds(bounds, min, max);
    }

    match &entity.geometry {
        SceneGeometry::Insert {
            block_name,
            transform: insert_transform,
        } => {
            if block_stack.contains(block_name) {
                return;
            }
            block_stack.push(block_name.clone());
            let composed = transform.compose(Affine2::from_trs(
                insert_transform.position.x,
                insert_transform.position.y,
                insert_transform.scale_x,
                insert_transform.scale_y,
                insert_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                for &child_id in children {
                    collect_entity_bounds_recursive(
                        drawing,
                        visible_entity_ids,
                        child_id,
                        composed,
                        depth + 1,
                        block_stack,
                        bounds,
                    );
                }
            }
            block_stack.pop();
        }
        SceneGeometry::Dimension {
            block_name,
            transform: dim_transform,
        } => {
            if block_name.is_empty() || block_stack.contains(block_name) {
                return;
            }
            block_stack.push(block_name.clone());
            let composed = transform.compose(Affine2::from_trs(
                dim_transform.position.x,
                dim_transform.position.y,
                dim_transform.scale_x,
                dim_transform.scale_y,
                dim_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                for &child_id in children {
                    collect_entity_bounds_recursive(
                        drawing,
                        visible_entity_ids,
                        child_id,
                        composed,
                        depth + 1,
                        block_stack,
                        bounds,
                    );
                }
            }
            block_stack.pop();
        }
        _ => {}
    }
}

fn include_bounds(bounds: &mut Option<Bounds2D>, min: Point2, max: Point2) {
    match bounds {
        Some(active) => {
            active.include_point(min);
            active.include_point(max);
        }
        None => {
            let mut initial = Bounds2D::from_point(min);
            initial.include_point(max);
            *bounds = Some(initial);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_entity_recursive(
    painter: &Painter,
    viewport: &ViewportState,
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    entity_id: usize,
    transform: Affine2,
    depth: usize,
    block_stack: &mut Vec<String>,
    selected_entity: Option<usize>,
    selected_transform_chain: &mut Vec<String>,
    selected_style_source: &mut Option<crate::extraction::models::StyleSource>,
    clip_rect: Rect,
    stats: &mut RenderStats,
) {
    if depth > MAX_RECURSION_DEPTH {
        return;
    }
    if !visible_entity_ids.contains(&entity_id) {
        return;
    }
    let entity = &drawing.entities[entity_id];
    stats.traversed_entities += 1;
    let style = resolve_style(entity, drawing);

    if selected_entity == Some(entity.id) {
        *selected_transform_chain = block_stack.clone();
        selected_transform_chain.push(format!("Entity:{:X}", entity.handle));
        *selected_style_source = Some(style.color_source);
    }

    let selected = selected_entity == Some(entity.id);
    if draw_primitive_entity(painter, viewport, entity, transform, &style, selected, clip_rect) {
        stats.drawn_entities += 1;
    } else {
        stats.culled_entities += 1;
    }

    match &entity.geometry {
        SceneGeometry::Insert {
            block_name,
            transform: insert_transform,
        } => {
            if block_stack.contains(block_name) {
                return;
            }
            block_stack.push(block_name.clone());
            let composed = transform.compose(Affine2::from_trs(
                insert_transform.position.x,
                insert_transform.position.y,
                insert_transform.scale_x,
                insert_transform.scale_y,
                insert_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                let mut ordered_children = children.clone();
                ordered_children.sort_by_key(|id| {
                    let is_hatch = matches!(drawing.entities[*id].geometry, SceneGeometry::Hatch { .. });
                    (is_hatch as u8, *id)
                });
                for child_id in ordered_children {
                    draw_entity_recursive(
                        painter,
                        viewport,
                        drawing,
                        visible_entity_ids,
                        child_id,
                        composed,
                        depth + 1,
                        block_stack,
                        selected_entity,
                        selected_transform_chain,
                        selected_style_source,
                        clip_rect,
                        stats,
                    );
                }
            }
            block_stack.pop();
        }
        SceneGeometry::Dimension {
            block_name,
            transform: dim_transform,
        } => {
            if block_name.is_empty() || block_stack.contains(block_name) {
                return;
            }
            block_stack.push(block_name.clone());
            let composed = transform.compose(Affine2::from_trs(
                dim_transform.position.x,
                dim_transform.position.y,
                dim_transform.scale_x,
                dim_transform.scale_y,
                dim_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                for child_id in children {
                    draw_entity_recursive(
                        painter,
                        viewport,
                        drawing,
                        visible_entity_ids,
                        *child_id,
                        composed,
                        depth + 1,
                        block_stack,
                        selected_entity,
                        selected_transform_chain,
                        selected_style_source,
                        clip_rect,
                        stats,
                    );
                }
            }
            block_stack.pop();
        }
        _ => {}
    }
}

fn draw_primitive_entity(
    painter: &Painter,
    viewport: &ViewportState,
    entity: &SceneEntity,
    transform: Affine2,
    style: &ResolvedStyle,
    selected: bool,
    clip_rect: Rect,
) -> bool {
    let stroke = if selected {
        Stroke::new((style.line_width + 1.0).min(4.0), Color32::YELLOW)
    } else {
        Stroke::new(style.line_width, style.color)
    };
    let transformed_bounds = transformed_bounds(entity, transform);
    if let Some(bounds) = transformed_bounds {
        if !intersects_clip(viewport, bounds, clip_rect) {
            return false;
        }
    }

    match &entity.geometry {
        SceneGeometry::Line { start, end } => {
            let start_screen = viewport.world_to_screen(transform.transform_point(*start));
            let end_screen = viewport.world_to_screen(transform.transform_point(*end));
            painter.line_segment([start_screen, end_screen], stroke);
        }
        SceneGeometry::Circle { center, radius } => {
            let segments = circle_render_segments(viewport, transform, *radius);
            let points = sample_circle_screen_points(viewport, transform, *center, *radius, segments);
            painter.add(egui::Shape::line(points, stroke));
        }
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let segments = arc_render_segments(viewport, transform, *radius, *start_angle, *end_angle);
            let points = sample_arc_screen_points(
                viewport,
                transform,
                *center,
                *radius,
                *start_angle,
                *end_angle,
                segments,
            );
            painter.add(egui::Shape::line(points, stroke));
        }
        SceneGeometry::Ellipse {
            center,
            major_axis,
            minor_axis_ratio,
            start_parameter,
            end_parameter,
        } => {
            let segments = ellipse_render_segments(
                viewport,
                transform,
                *major_axis,
                *minor_axis_ratio,
                *start_parameter,
                *end_parameter,
            );
            let points = sample_ellipse_screen_points(
                viewport,
                transform,
                *center,
                *major_axis,
                *minor_axis_ratio,
                *start_parameter,
                *end_parameter,
                segments,
            );
            painter.add(egui::Shape::line(points, stroke));
        }
        SceneGeometry::LwPolyline { polyline } | SceneGeometry::Polyline2D { polyline } => {
            let points = flatten_polyline2d_for_render(viewport, transform, polyline);
            if points.len() >= 2 {
                let mut path: Vec<Pos2> = points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                if polyline.closed {
                    path.push(viewport.world_to_screen(transform.transform_point(points[0])));
                }
                painter.add(egui::Shape::line(path, stroke));
            }
        }
        SceneGeometry::Polyline3D { polyline } => {
            let points = &polyline.vertices;
            if points.len() >= 2 {
                let mut path: Vec<Pos2> = points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                if polyline.closed {
                    path.push(viewport.world_to_screen(transform.transform_point(points[0])));
                }
                painter.add(egui::Shape::line(path, stroke));
            }
        }
        SceneGeometry::Spline { spline } => {
            let points = sample_spline_geometry_for_render(viewport, transform, spline);
            if points.len() >= 2 {
                let sampled = points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                painter.add(egui::Shape::line(sampled, stroke));
            }
        }
        SceneGeometry::Solid { points } => {
            if points.len() >= 3 {
                let transformed: Vec<Pos2> = points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                painter.add(egui::Shape::convex_polygon(
                    transformed,
                    style.color.gamma_multiply(0.22),
                    stroke,
                ));
            }
        }
        SceneGeometry::Hatch { paths, solid_fill } => {
            for path in paths {
                let path_points = sample_hatch_path_points_for_render(viewport, transform, path);
                if path_points.len() < 3 {
                    continue;
                }
                let transformed: Vec<Pos2> = path_points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                if *solid_fill {
                    painter.add(egui::Shape::convex_polygon(
                        transformed,
                        style.color.gamma_multiply(0.16),
                        stroke,
                    ));
                } else {
                    painter.add(egui::Shape::line(transformed, stroke));
                }
            }
        }
        SceneGeometry::Text { position, payload } => {
            let pos = viewport.world_to_screen(transform.transform_point(*position));
            let size = payload.height as f32 * viewport.zoom * transform.scale_hint() as f32 * 1.25;
            
            if size > 1.5 {
                let size = size.clamp(1.5, 2000.0);
                let galley = painter.layout_no_wrap(
                    decode_cad_text(&payload.value),
                    FontId::new(size, FontFamily::Proportional),
                    style.color,
                );

                let base_angle = transform.rotation_hint() as f32;
                let screen_angle = -(base_angle + payload.rotation as f32);

                let rot = egui::emath::Rot2::from_angle(screen_angle);
                let text_pos = pos - rot * egui::Vec2::new(0.0, galley.size().y);

                let mut text_shape = egui::epaint::TextShape::new(text_pos, galley, style.color);
                text_shape.angle = screen_angle;

                painter.add(text_shape);
            }
        }
        SceneGeometry::Insert {
            transform: entity_transform,
            ..
        }
        | SceneGeometry::Dimension {
            transform: entity_transform,
            ..
        } => {
            let center =
                viewport.world_to_screen(transform.transform_point(entity_transform.position));
            painter.circle_filled(center, 2.0, style.color.gamma_multiply(0.6));
        }
        SceneGeometry::Unsupported { .. } => return false,
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn pick_entity_recursive(
    drawing: &ExtractedDrawing,
    viewport: &ViewportState,
    click_pos: Pos2,
    visible_entity_ids: &BTreeSet<usize>,
    entity_id: usize,
    transform: Affine2,
    depth: usize,
    closest: &mut Option<(usize, f32)>,
) {
    if depth > MAX_RECURSION_DEPTH || !visible_entity_ids.contains(&entity_id) {
        return;
    }
    let entity = &drawing.entities[entity_id];
    let distance = distance_to_entity_px(viewport, click_pos, entity, transform);
    match *closest {
        Some((_, best_distance)) if distance >= best_distance => {}
        _ => *closest = Some((entity.id, distance)),
    }

    match &entity.geometry {
        SceneGeometry::Insert {
            block_name,
            transform: child_transform,
        } => {
            let composed = transform.compose(Affine2::from_trs(
                child_transform.position.x,
                child_transform.position.y,
                child_transform.scale_x,
                child_transform.scale_y,
                child_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                for child_id in children {
                    pick_entity_recursive(
                        drawing,
                        viewport,
                        click_pos,
                        visible_entity_ids,
                        *child_id,
                        composed,
                        depth + 1,
                        closest,
                    );
                }
            }
        }
        SceneGeometry::Dimension {
            block_name,
            transform: child_transform,
        } => {
            let composed = transform.compose(Affine2::from_trs(
                child_transform.position.x,
                child_transform.position.y,
                child_transform.scale_x,
                child_transform.scale_y,
                child_transform.rotation,
            ));
            if let Some(children) = drawing.block_index.get(block_name) {
                for child_id in children {
                    pick_entity_recursive(
                        drawing,
                        viewport,
                        click_pos,
                        visible_entity_ids,
                        *child_id,
                        composed,
                        depth + 1,
                        closest,
                    );
                }
            }
        }
        _ => {}
    }
}

fn distance_to_entity_px(
    viewport: &ViewportState,
    click_pos: Pos2,
    entity: &SceneEntity,
    transform: Affine2,
) -> f32 {
    match &entity.geometry {
        SceneGeometry::Line { start, end } => {
            let a = viewport.world_to_screen(transform.transform_point(*start));
            let b = viewport.world_to_screen(transform.transform_point(*end));
            distance_to_segment(click_pos, a, b)
        }
        SceneGeometry::Circle { center, radius } => {
            let segments = circle_render_segments(viewport, transform, *radius);
            let points = sample_circle_screen_points(viewport, transform, *center, *radius, segments);
            distance_to_polyline(click_pos, &points, true)
        }
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let segments = arc_render_segments(viewport, transform, *radius, *start_angle, *end_angle);
            let points = sample_arc_screen_points(
                viewport,
                transform,
                *center,
                *radius,
                *start_angle,
                *end_angle,
                segments,
            );
            distance_to_polyline(click_pos, &points, false)
        }
        SceneGeometry::LwPolyline { polyline } | SceneGeometry::Polyline2D { polyline } => {
            let points = flatten_polyline2d_for_render(viewport, transform, polyline);
            if points.len() < 2 {
                return f32::MAX;
            }
            let mut best = f32::MAX;
            for segment in points.windows(2) {
                let a = viewport.world_to_screen(transform.transform_point(segment[0]));
                let b = viewport.world_to_screen(transform.transform_point(segment[1]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            if polyline.closed {
                let a =
                    viewport.world_to_screen(transform.transform_point(points[points.len() - 1]));
                let b = viewport.world_to_screen(transform.transform_point(points[0]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            best
        }
        SceneGeometry::Polyline3D { polyline } => {
            let points = &polyline.vertices;
            if points.len() < 2 {
                return f32::MAX;
            }
            let mut best = f32::MAX;
            for segment in points.windows(2) {
                let a = viewport.world_to_screen(transform.transform_point(segment[0]));
                let b = viewport.world_to_screen(transform.transform_point(segment[1]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            if polyline.closed {
                let a =
                    viewport.world_to_screen(transform.transform_point(points[points.len() - 1]));
                let b = viewport.world_to_screen(transform.transform_point(points[0]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            best
        }
        SceneGeometry::Ellipse {
            center,
            major_axis,
            minor_axis_ratio,
            start_parameter,
            end_parameter,
        } => {
            let segments = ellipse_render_segments(
                viewport,
                transform,
                *major_axis,
                *minor_axis_ratio,
                *start_parameter,
                *end_parameter,
            );
            let points = sample_ellipse_screen_points(
                viewport,
                transform,
                *center,
                *major_axis,
                *minor_axis_ratio,
                *start_parameter,
                *end_parameter,
                segments,
            );
            distance_to_polyline(click_pos, &points, false)
        }
        SceneGeometry::Spline { spline } => {
            let points = sample_spline_geometry_for_render(viewport, transform, spline);
            if points.len() < 2 {
                return f32::MAX;
            }
            let mut best = f32::MAX;
            for segment in points.windows(2) {
                let a = viewport.world_to_screen(transform.transform_point(segment[0]));
                let b = viewport.world_to_screen(transform.transform_point(segment[1]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            best
        }
        SceneGeometry::Solid { points } => {
            points
                .iter()
                .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                .map(|point| point.distance(click_pos))
                .fold(f32::MAX, f32::min)
        }
        SceneGeometry::Hatch { paths, .. } => paths
            .iter()
            .flat_map(|path| sample_hatch_path_points_for_render(viewport, transform, path))
            .map(|point| viewport.world_to_screen(transform.transform_point(point)))
            .map(|point| point.distance(click_pos))
            .fold(f32::MAX, f32::min),
        SceneGeometry::Text { position, .. } => {
            let marker = viewport.world_to_screen(transform.transform_point(*position));
            marker.distance(click_pos)
        }
        SceneGeometry::Insert {
            transform: entity_transform,
            ..
        }
        | SceneGeometry::Dimension {
            transform: entity_transform,
            ..
        } => {
            let marker =
                viewport.world_to_screen(transform.transform_point(entity_transform.position));
            marker.distance(click_pos)
        }
        SceneGeometry::Unsupported { .. } => f32::MAX,
    }
}

fn distance_to_segment(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let len_sq = segment.length_sq();
    if len_sq <= f32::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / len_sq).clamp(0.0, 1.0);
    let projection = start + segment * t;
    point.distance(projection)
}

fn nearly_same_point(a: Point2, b: Point2) -> bool {
    (a.x - b.x).abs() <= 1e-9 && (a.y - b.y).abs() <= 1e-9
}

fn flatten_polyline2d(polyline: &Polyline2DGeometry) -> Vec<Point2> {
    if polyline.vertices.is_empty() {
        return Vec::new();
    }

    let vertices = polyline
        .vertices
        .iter()
        .map(|vertex| (vertex.location, vertex.bulge))
        .collect::<Vec<_>>();
    expand_bulged_polyline(&vertices, polyline.closed, |_, _, arc| {
        bounds_segments_for_sweep(arc.sweep)
    })
}

fn flatten_polyline2d_for_render(
    viewport: &ViewportState,
    transform: Affine2,
    polyline: &Polyline2DGeometry,
) -> Vec<Point2> {
    if polyline.vertices.is_empty() {
        return Vec::new();
    }

    let vertices = polyline
        .vertices
        .iter()
        .map(|vertex| (vertex.location, vertex.bulge))
        .collect::<Vec<_>>();
    expand_bulged_polyline(&vertices, polyline.closed, |start, end, arc| {
        bulge_render_segments(viewport, transform, start, end, arc)
    })
}

fn expand_bulged_polyline<F>(vertices: &[(Point2, f64)], closed: bool, segment_resolver: F) -> Vec<Point2>
where
    F: Fn(Point2, Point2, &BulgeArc) -> usize,
{
    if vertices.is_empty() {
        return Vec::new();
    }

    let mut points = vec![vertices[0].0];
    let segment_count = if closed {
        vertices.len()
    } else {
        vertices.len().saturating_sub(1)
    };

    for i in 0..segment_count {
        let next = (i + 1) % vertices.len();
        append_bulged_segment(
            &mut points,
            vertices[i].0,
            vertices[next].0,
            vertices[i].1,
            &segment_resolver,
        );
    }

    points
}

fn append_bulged_segment<F>(
    points: &mut Vec<Point2>,
    start: Point2,
    end: Point2,
    bulge: f64,
    segment_resolver: &F,
) where
    F: Fn(Point2, Point2, &BulgeArc) -> usize,
{
    if nearly_same_point(start, end) {
        if points.last().copied() != Some(end) {
            points.push(end);
        }
        return;
    }

    if bulge.abs() <= 1e-9 {
        points.push(end);
        return;
    }

    let Some(arc) = bulge_arc_from_segment(start, end, bulge) else {
        points.push(end);
        return;
    };

    let segments = segment_resolver(start, end, &arc).max(1);
    for step in 1..=segments {
        let t = step as f64 / segments as f64;
        let angle = arc.start_angle + arc.sweep * t;
        points.push(Point2::new(
            arc.center.x + arc.radius * angle.cos(),
            arc.center.y + arc.radius * angle.sin(),
        ));
    }

    if let Some(last) = points.last_mut() {
        *last = end;
    }
}

#[derive(Clone, Copy)]
struct BulgeArc {
    center: Point2,
    radius: f64,
    start_angle: f64,
    sweep: f64,
}

fn bulge_arc_from_segment(start: Point2, end: Point2, bulge: f64) -> Option<BulgeArc> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = (dx * dx + dy * dy).sqrt();
    if chord <= 1e-9 {
        return None;
    }

    let sweep = 4.0 * bulge.atan();
    if sweep.abs() <= 1e-9 {
        return None;
    }

    let radius = chord / (2.0 * (sweep * 0.5).sin().abs());
    let midpoint = Point2::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
    let left_normal = Point2::new(-dy / chord, dx / chord);
    let center_offset = chord / (2.0 * (sweep * 0.5).tan());
    let center = Point2::new(
        midpoint.x + left_normal.x * center_offset,
        midpoint.y + left_normal.y * center_offset,
    );
    let start_angle = (start.y - center.y).atan2(start.x - center.x);

    Some(BulgeArc {
        center,
        radius,
        start_angle,
        sweep,
    })
}

fn bulge_render_segments(
    viewport: &ViewportState,
    transform: Affine2,
    start: Point2,
    _end: Point2,
    arc: &BulgeArc,
) -> usize {
    let radius_vector = Point2::new(start.x - arc.center.x, start.y - arc.center.y);
    let tangent_radius = Point2::new(-radius_vector.y, radius_vector.x);
    let screen_radius = projected_vector_pixels(viewport, transform, radius_vector)
        .max(projected_vector_pixels(viewport, transform, tangent_radius));
    render_segments_for_radius(screen_radius, arc.sweep)
}

fn sample_spline_geometry(spline: &SplineGeometry) -> Vec<Point2> {
    sample_spline_geometry_with_target(spline, MIN_CURVE_SEGMENTS * 6)
}

fn sample_spline_geometry_for_render(
    viewport: &ViewportState,
    transform: Affine2,
    spline: &SplineGeometry,
) -> Vec<Point2> {
    let reference_points = if spline.control_points.len() >= 2 {
        &spline.control_points
    } else {
        &spline.fit_points
    };
    let approx_length = approximate_polyline_screen_length(
        viewport,
        transform,
        reference_points,
        spline.flags.closed,
    );
    let target_segments =
        (approx_length / CURVE_TARGET_SEGMENT_PIXELS).ceil() as usize;
    sample_spline_geometry_with_target(
        spline,
        target_segments.clamp(MIN_CURVE_SEGMENTS, MAX_CURVE_SEGMENTS),
    )
}

fn sample_spline_geometry_with_target(spline: &SplineGeometry, target_segments: usize) -> Vec<Point2> {
    let degree = spline.degree.max(1) as usize;

    if spline.control_points.len() >= degree + 1 {
        let knots = if spline.knots.len() >= spline.control_points.len() + degree + 1 {
            spline.knots.clone()
        } else {
            acadrust::Spline::generate_clamped_knots(degree, spline.control_points.len())
        };

        if let Some(sampled) = sample_nurbs_curve(
            &spline.control_points,
            if spline.weights.len() == spline.control_points.len() {
                Some(&spline.weights)
            } else {
                None
            },
            &knots,
            degree,
            spline.flags.closed,
            target_segments,
        ) {
            return sampled;
        }
    }

    if spline.fit_points.len() >= 2 {
        let segment_count = if spline.flags.closed {
            spline.fit_points.len()
        } else {
            spline.fit_points.len().saturating_sub(1)
        }
        .max(1);
        let subdivisions = ((target_segments as f64 / segment_count as f64).ceil() as usize)
            .clamp(4, MAX_CURVE_SEGMENTS);
        return sample_catmull_rom(&spline.fit_points, spline.flags.closed, subdivisions);
    }

    if !spline.control_points.is_empty() {
        spline.control_points.clone()
    } else {
        spline.fit_points.clone()
    }
}

fn sample_catmull_rom(points: &[Point2], closed: bool, subdivisions: usize) -> Vec<Point2> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };

    let mut sampled = Vec::new();
    for i in 0..segment_count {
        let p0 = if i == 0 {
            if closed { points[points.len() - 1] } else { points[0] }
        } else {
            points[i - 1]
        };
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        let p3 = if i + 2 >= points.len() {
            if closed {
                points[(i + 2) % points.len()]
            } else {
                points[points.len() - 1]
            }
        } else {
            points[i + 2]
        };

        if sampled.is_empty() {
            sampled.push(p1);
        }

        for step in 1..=subdivisions {
            let t = step as f64 / subdivisions as f64;
            sampled.push(catmull_rom_point(p0, p1, p2, p3, t));
        }
    }

    if !closed && sampled.last().copied() != points.last().copied() {
        sampled.push(points[points.len() - 1]);
    }

    sampled
}

fn catmull_rom_point(p0: Point2, p1: Point2, p2: Point2, p3: Point2, t: f64) -> Point2 {
    let t2 = t * t;
    let t3 = t2 * t;
    Point2::new(
        0.5
            * ((2.0 * p1.x)
                + (-p0.x + p2.x) * t
                + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
        0.5
            * ((2.0 * p1.y)
                + (-p0.y + p2.y) * t
                + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
    )
}

fn sample_nurbs_curve(
    control_points: &[Point2],
    weights: Option<&[f64]>,
    knots: &[f64],
    degree: usize,
    closed: bool,
    target_segments: usize,
) -> Option<Vec<Point2>> {
    if control_points.len() < degree + 1 || knots.len() < control_points.len() + degree + 1 {
        return None;
    }

    let start = *knots.get(degree)?;
    let end = *knots.get(knots.len().checked_sub(degree + 1)?)?;
    if end <= start {
        return None;
    }

    let n = control_points.len() - 1;
    let mut sampled = Vec::new();
    let mut appended = false;
    let target_segments = target_segments.clamp(MIN_CURVE_SEGMENTS, MAX_CURVE_SEGMENTS);

    for span in degree..=n {
        let u0 = knots[span];
        let u1 = knots[span + 1];
        if u1 <= u0 {
            continue;
        }

        let steps = ((((u1 - u0) / (end - start)) * target_segments as f64).ceil() as usize)
            .max(1);
        for step in 0..=steps {
            if appended && step == 0 {
                continue;
            }
            let t = step as f64 / steps as f64;
            let u = if span == n && step == steps {
                end
            } else {
                u0 + (u1 - u0) * t
            };
            sampled.push(evaluate_nurbs_point(
                control_points,
                weights,
                knots,
                degree,
                u,
            )?);
            appended = true;
        }
    }

    if closed && sampled.first().copied() != sampled.last().copied() {
        sampled.push(sampled[0]);
    }

    Some(sampled)
}

fn evaluate_nurbs_point(
    control_points: &[Point2],
    weights: Option<&[f64]>,
    knots: &[f64],
    degree: usize,
    u: f64,
) -> Option<Point2> {
    let n = control_points.len().checked_sub(1)?;
    if degree > n {
        return None;
    }

    let last_domain = knots.get(n + 1).copied()?;
    if (u - last_domain).abs() <= 1e-9 {
        return control_points.last().copied();
    }

    let span = find_knot_span(control_points.len(), degree, knots, u)?;
    let mut points = (0..=degree)
        .map(|j| {
            let index = span - degree + j;
            let weight = weights
                .and_then(|values| values.get(index))
                .copied()
                .unwrap_or(1.0);
            HomPoint {
                xw: control_points[index].x * weight,
                yw: control_points[index].y * weight,
                w: weight,
            }
        })
        .collect::<Vec<_>>();

    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let index = span - degree + j;
            let denom = knots[index + degree + 1 - r] - knots[index];
            let alpha = if denom.abs() <= 1e-12 {
                0.0
            } else {
                (u - knots[index]) / denom
            };
            points[j] = points[j - 1].lerp(points[j], alpha);
        }
    }

    points[degree].to_point()
}

fn find_knot_span(
    control_point_count: usize,
    degree: usize,
    knots: &[f64],
    u: f64,
) -> Option<usize> {
    let n = control_point_count.checked_sub(1)?;
    let low = degree;
    let high = n + 1;
    if u < knots[low] || u > knots[high] {
        return None;
    }

    let mut left = low;
    let mut right = high;
    while left + 1 < right {
        let mid = (left + right) / 2;
        if u < knots[mid] {
            right = mid;
        } else {
            left = mid;
        }
    }
    Some(left.min(n))
}

#[derive(Clone, Copy)]
struct HomPoint {
    xw: f64,
    yw: f64,
    w: f64,
}

impl HomPoint {
    fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            xw: self.xw + (other.xw - self.xw) * t,
            yw: self.yw + (other.yw - self.yw) * t,
            w: self.w + (other.w - self.w) * t,
        }
    }

    fn to_point(self) -> Option<Point2> {
        if self.w.abs() <= 1e-12 {
            None
        } else {
            Some(Point2::new(self.xw / self.w, self.yw / self.w))
        }
    }
}

fn sample_hatch_path_points(path: &HatchBoundaryPathGeometry) -> Vec<Point2> {
    let mut points = Vec::new();
    for edge in &path.edges {
        let edge_points = match edge {
            HatchBoundaryEdgeGeometry::Line { start, end } => vec![*start, *end],
            HatchBoundaryEdgeGeometry::CircularArc {
                center,
                radius,
                start_angle,
                end_angle,
                counter_clockwise,
            } => sample_arc_points(
                *center,
                *radius,
                *start_angle,
                *end_angle,
                *counter_clockwise,
            ),
            HatchBoundaryEdgeGeometry::EllipticArc {
                center,
                major_axis_endpoint,
                minor_axis_ratio,
                start_angle,
                end_angle,
                counter_clockwise,
            } => sample_ellipse_points(
                *center,
                *major_axis_endpoint,
                *minor_axis_ratio,
                *start_angle,
                *end_angle,
                *counter_clockwise,
            ),
            HatchBoundaryEdgeGeometry::Spline(spline) => sample_spline_geometry(spline),
            HatchBoundaryEdgeGeometry::Polyline(polyline) => flatten_polyline2d(polyline),
        };
        append_path_points(&mut points, edge_points);
    }
    points
}

fn sample_hatch_path_points_for_render(
    viewport: &ViewportState,
    transform: Affine2,
    path: &HatchBoundaryPathGeometry,
) -> Vec<Point2> {
    let mut points = Vec::new();
    for edge in &path.edges {
        let edge_points = match edge {
            HatchBoundaryEdgeGeometry::Line { start, end } => vec![*start, *end],
            HatchBoundaryEdgeGeometry::CircularArc {
                center,
                radius,
                start_angle,
                end_angle,
                counter_clockwise,
            } => {
                let sweep = normalized_sweep(*start_angle, *end_angle, *counter_clockwise);
                let segments =
                    hatch_arc_render_segments(viewport, transform, *radius, *start_angle, sweep);
                sample_arc_points_with_segments(
                    *center,
                    *radius,
                    *start_angle,
                    sweep,
                    segments,
                )
            }
            HatchBoundaryEdgeGeometry::EllipticArc {
                center,
                major_axis_endpoint,
                minor_axis_ratio,
                start_angle,
                end_angle,
                counter_clockwise,
            } => {
                let sweep = normalized_sweep(*start_angle, *end_angle, *counter_clockwise);
                let segments = hatch_ellipse_render_segments(
                    viewport,
                    transform,
                    *major_axis_endpoint,
                    *minor_axis_ratio,
                    sweep,
                );
                sample_ellipse_points_with_segments(
                    *center,
                    *major_axis_endpoint,
                    *minor_axis_ratio,
                    *start_angle,
                    sweep,
                    segments,
                )
            }
            HatchBoundaryEdgeGeometry::Spline(spline) => {
                sample_spline_geometry_for_render(viewport, transform, spline)
            }
            HatchBoundaryEdgeGeometry::Polyline(polyline) => {
                flatten_polyline2d_for_render(viewport, transform, polyline)
            }
        };
        append_path_points(&mut points, edge_points);
    }
    points
}

fn append_path_points(points: &mut Vec<Point2>, edge_points: Vec<Point2>) {
    if edge_points.is_empty() {
        return;
    }

    if points.is_empty() {
        points.extend(edge_points);
        return;
    }

    if let Some(last) = points.last().copied() {
        if nearly_same_point(last, edge_points[0]) {
            points.extend(edge_points.into_iter().skip(1));
            return;
        }
    }

    points.extend(edge_points);
}

fn sample_arc_points(
    center: Point2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    counter_clockwise: bool,
) -> Vec<Point2> {
    let sweep = normalized_sweep(start_angle, end_angle, counter_clockwise);
    let segments = bounds_segments_for_sweep(sweep);
    sample_arc_points_with_segments(center, radius, start_angle, sweep, segments)
}

fn sample_arc_points_with_segments(
    center: Point2,
    radius: f64,
    start_angle: f64,
    sweep: f64,
    segments: usize,
) -> Vec<Point2> {
    let radius = radius.abs();

    (0..=segments)
        .map(|i| {
            let t = i as f64 / segments as f64;
            let angle = start_angle + sweep * t;
            Point2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

fn sample_ellipse_points(
    center: Point2,
    major_axis_endpoint: Point2,
    minor_axis_ratio: f64,
    start_angle: f64,
    end_angle: f64,
    counter_clockwise: bool,
) -> Vec<Point2> {
    let sweep = normalized_sweep(start_angle, end_angle, counter_clockwise);
    let segments = bounds_segments_for_sweep(sweep);
    sample_ellipse_points_with_segments(
        center,
        major_axis_endpoint,
        minor_axis_ratio,
        start_angle,
        sweep,
        segments,
    )
}

fn sample_ellipse_points_with_segments(
    center: Point2,
    major_axis_endpoint: Point2,
    minor_axis_ratio: f64,
    start_angle: f64,
    sweep: f64,
    segments: usize,
) -> Vec<Point2> {
    let major_len = (major_axis_endpoint.x * major_axis_endpoint.x
        + major_axis_endpoint.y * major_axis_endpoint.y)
        .sqrt()
        .max(1e-6);
    let minor_len = major_len * minor_axis_ratio;
    let axis_angle = major_axis_endpoint.y.atan2(major_axis_endpoint.x);

    (0..=segments)
        .map(|i| {
            let t = i as f64 / segments as f64;
            let angle = start_angle + sweep * t;
            let x = major_len * angle.cos();
            let y = minor_len * angle.sin();
            let rotated_x = x * axis_angle.cos() - y * axis_angle.sin();
            let rotated_y = x * axis_angle.sin() + y * axis_angle.cos();
            Point2::new(center.x + rotated_x, center.y + rotated_y)
        })
        .collect()
}

fn approximate_polyline_screen_length(
    viewport: &ViewportState,
    transform: Affine2,
    points: &[Point2],
    closed: bool,
) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }

    let mut length = 0.0;
    for segment in points.windows(2) {
        let a = viewport.world_to_screen(transform.transform_point(segment[0]));
        let b = viewport.world_to_screen(transform.transform_point(segment[1]));
        length += a.distance(b) as f64;
    }

    if closed {
        let a = viewport.world_to_screen(transform.transform_point(points[points.len() - 1]));
        let b = viewport.world_to_screen(transform.transform_point(points[0]));
        length += a.distance(b) as f64;
    }

    length
}

fn distance_to_polyline(point: Pos2, points: &[Pos2], closed: bool) -> f32 {
    if points.len() < 2 {
        return f32::MAX;
    }

    let mut best = f32::MAX;
    for segment in points.windows(2) {
        best = best.min(distance_to_segment(point, segment[0], segment[1]));
    }

    if closed {
        best = best.min(distance_to_segment(point, points[points.len() - 1], points[0]));
    }

    best
}

fn normalize_arc_sweep(start: f64, end: f64) -> (f64, f64) {
    let mut normalized_end = end;
    while normalized_end < start {
        normalized_end += std::f64::consts::TAU;
    }
    (start, normalized_end)
}

fn normalized_sweep(start_angle: f64, end_angle: f64, counter_clockwise: bool) -> f64 {
    let mut sweep = end_angle - start_angle;
    if counter_clockwise {
        while sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }
    } else {
        while sweep > 0.0 {
            sweep -= std::f64::consts::TAU;
        }
    }
    sweep
}

fn projected_vector_pixels(viewport: &ViewportState, transform: Affine2, vector: Point2) -> f64 {
    let x = transform.m11 * vector.x + transform.m12 * vector.y;
    let y = transform.m21 * vector.x + transform.m22 * vector.y;
    (x * x + y * y).sqrt() * viewport.zoom as f64
}

fn render_segments_for_radius(screen_radius: f64, sweep_radians: f64) -> usize {
    ((screen_radius.max(1.0) * sweep_radians.abs().max(1e-6) / CURVE_TARGET_SEGMENT_PIXELS).ceil()
        as usize)
        .clamp(MIN_CURVE_SEGMENTS, MAX_CURVE_SEGMENTS)
}

fn circle_render_segments(viewport: &ViewportState, transform: Affine2, radius: f64) -> usize {
    let radius = radius.abs();
    let screen_radius = projected_vector_pixels(viewport, transform, Point2::new(radius, 0.0))
        .max(projected_vector_pixels(
            viewport,
            transform,
            Point2::new(0.0, radius),
        ));
    render_segments_for_radius(screen_radius, std::f64::consts::TAU)
}

fn arc_render_segments(
    viewport: &ViewportState,
    transform: Affine2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
) -> usize {
    let (start_angle, end_angle) = normalize_arc_sweep(start_angle, end_angle);
    let radius = radius.abs();
    let screen_radius = projected_vector_pixels(viewport, transform, Point2::new(radius, 0.0))
        .max(projected_vector_pixels(
            viewport,
            transform,
            Point2::new(0.0, radius),
        ));
    render_segments_for_radius(screen_radius, end_angle - start_angle)
}

fn ellipse_render_segments(
    viewport: &ViewportState,
    transform: Affine2,
    major_axis: Point2,
    minor_axis_ratio: f64,
    start_parameter: f64,
    end_parameter: f64,
) -> usize {
    let (_, normalized_end) = normalize_arc_sweep(start_parameter, end_parameter);
    let major_screen = projected_vector_pixels(viewport, transform, major_axis);
    let minor_axis = Point2::new(-major_axis.y * minor_axis_ratio, major_axis.x * minor_axis_ratio);
    let minor_screen = projected_vector_pixels(viewport, transform, minor_axis);
    render_segments_for_radius(major_screen.max(minor_screen), normalized_end - start_parameter)
}

fn hatch_arc_render_segments(
    viewport: &ViewportState,
    transform: Affine2,
    radius: f64,
    start_angle: f64,
    sweep: f64,
) -> usize {
    let radius = radius.abs();
    let start_vector = Point2::new(radius * start_angle.cos(), radius * start_angle.sin());
    let tangent_vector = Point2::new(-start_vector.y, start_vector.x);
    let screen_radius = projected_vector_pixels(viewport, transform, start_vector)
        .max(projected_vector_pixels(viewport, transform, tangent_vector));
    render_segments_for_radius(screen_radius, sweep)
}

fn hatch_ellipse_render_segments(
    viewport: &ViewportState,
    transform: Affine2,
    major_axis: Point2,
    minor_axis_ratio: f64,
    sweep: f64,
) -> usize {
    let major_screen = projected_vector_pixels(viewport, transform, major_axis);
    let minor_axis = Point2::new(-major_axis.y * minor_axis_ratio, major_axis.x * minor_axis_ratio);
    let minor_screen = projected_vector_pixels(viewport, transform, minor_axis);
    render_segments_for_radius(major_screen.max(minor_screen), sweep)
}

fn bounds_segments_for_sweep(sweep_radians: f64) -> usize {
    ((sweep_radians.abs() / BOUNDS_CURVE_STEP_RADIANS).ceil() as usize)
        .clamp(MIN_CURVE_SEGMENTS * 2, MAX_CURVE_SEGMENTS)
}

fn sample_circle_screen_points(
    viewport: &ViewportState,
    transform: Affine2,
    center: Point2,
    radius: f64,
    segments: usize,
) -> Vec<Pos2> {
    let mut points = sample_circle_world_points(transform, center, radius, segments)
        .into_iter()
        .map(|point| viewport.world_to_screen(point))
        .collect::<Vec<_>>();
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}

fn sample_arc_screen_points(
    viewport: &ViewportState,
    transform: Affine2,
    center: Point2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    segments: usize,
) -> Vec<Pos2> {
    sample_arc_world_points(transform, center, radius, start_angle, end_angle, segments)
        .into_iter()
        .map(|point| viewport.world_to_screen(point))
        .collect()
}

fn sample_ellipse_screen_points(
    viewport: &ViewportState,
    transform: Affine2,
    center: Point2,
    major_axis: Point2,
    minor_axis_ratio: f64,
    start_parameter: f64,
    end_parameter: f64,
    segments: usize,
) -> Vec<Pos2> {
    sample_ellipse_world_points(
        transform,
        center,
        major_axis,
        minor_axis_ratio,
        start_parameter,
        end_parameter,
        segments,
    )
    .into_iter()
    .map(|point| viewport.world_to_screen(point))
    .collect()
}

fn sample_circle_world_points(
    transform: Affine2,
    center: Point2,
    radius: f64,
    segments: usize,
) -> Vec<Point2> {
    let radius = radius.abs();
    (0..segments)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / segments as f64;
            let point = Point2::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            );
            transform.transform_point(point)
        })
        .collect()
}

fn sample_arc_world_points(
    transform: Affine2,
    center: Point2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    segments: usize,
) -> Vec<Point2> {
    let radius = radius.abs();
    let (start_angle, end_angle) = normalize_arc_sweep(start_angle, end_angle);
    (0..=segments)
        .map(|i| {
            let t = i as f64 / segments as f64;
            let angle = start_angle + (end_angle - start_angle) * t;
            let point = Point2::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            );
            transform.transform_point(point)
        })
        .collect()
}

fn sample_ellipse_world_points(
    transform: Affine2,
    center: Point2,
    major_axis: Point2,
    minor_axis_ratio: f64,
    start_parameter: f64,
    end_parameter: f64,
    segments: usize,
) -> Vec<Point2> {
    let major_len = (major_axis.x * major_axis.x + major_axis.y * major_axis.y)
        .sqrt()
        .max(1e-6);
    let minor_len = major_len * minor_axis_ratio;
    let axis_angle = major_axis.y.atan2(major_axis.x);

    (0..=segments)
        .map(|i| {
            let t = i as f64 / segments as f64;
            let theta = start_parameter + (end_parameter - start_parameter) * t;
            let x = major_len * theta.cos();
            let y = minor_len * theta.sin();
            let rotated = Point2::new(
                center.x + x * axis_angle.cos() - y * axis_angle.sin(),
                center.y + x * axis_angle.sin() + y * axis_angle.cos(),
            );
            transform.transform_point(rotated)
        })
        .collect()
}

fn transformed_bounds(entity: &SceneEntity, transform: Affine2) -> Option<(Point2, Point2)> {
    match &entity.geometry {
        SceneGeometry::Circle { center, radius } => {
            let transformed_center = transform.transform_point(*center);
            let radius = radius.abs();
            let extent_x =
                radius * (transform.m11 * transform.m11 + transform.m12 * transform.m12).sqrt();
            let extent_y =
                radius * (transform.m21 * transform.m21 + transform.m22 * transform.m22).sqrt();
            return Some((
                Point2::new(
                    transformed_center.x - extent_x,
                    transformed_center.y - extent_y,
                ),
                Point2::new(
                    transformed_center.x + extent_x,
                    transformed_center.y + extent_y,
                ),
            ));
        }
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let segments = bounds_segments_for_sweep(normalize_arc_sweep(*start_angle, *end_angle).1 - *start_angle);
            return sampled_bounds(sample_arc_world_points(
                transform,
                *center,
                *radius,
                *start_angle,
                *end_angle,
                segments,
            ));
        }
        SceneGeometry::Ellipse {
            center,
            major_axis,
            minor_axis_ratio,
            start_parameter,
            end_parameter,
        } => {
            let segments =
                bounds_segments_for_sweep(normalize_arc_sweep(*start_parameter, *end_parameter).1 - *start_parameter);
            return sampled_bounds(sample_ellipse_world_points(
                transform,
                *center,
                *major_axis,
                *minor_axis_ratio,
                *start_parameter,
                *end_parameter,
                segments,
            ));
        }
        SceneGeometry::LwPolyline { polyline } | SceneGeometry::Polyline2D { polyline } => {
            return sampled_bounds(
                flatten_polyline2d(polyline)
                    .into_iter()
                    .map(|point| transform.transform_point(point))
                    .collect(),
            );
        }
        SceneGeometry::Spline { spline } => {
            return sampled_bounds(
                sample_spline_geometry(spline)
                    .into_iter()
                    .map(|point| transform.transform_point(point))
                    .collect(),
            );
        }
        SceneGeometry::Hatch { paths, .. } => {
            return sampled_bounds(
                paths.iter()
                    .flat_map(sample_hatch_path_points)
                    .map(|point| transform.transform_point(point))
                    .collect(),
            );
        }
        _ => {}
    }

    let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut has_point = false;
    entity.geometry.visit_points(|point| {
        has_point = true;
        let transformed = transform.transform_point(point);
        min.x = min.x.min(transformed.x);
        min.y = min.y.min(transformed.y);
        max.x = max.x.max(transformed.x);
        max.y = max.y.max(transformed.y);
    });
    if has_point { Some((min, max)) } else { None }
}

fn sampled_bounds(points: Vec<Point2>) -> Option<(Point2, Point2)> {
    let mut iter = points.into_iter();
    let first = iter.next()?;
    let mut min = first;
    let mut max = first;
    for point in iter {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Some((min, max))
}

fn intersects_clip(
    viewport: &ViewportState,
    bounds: (Point2, Point2),
    clip_rect: Rect,
) -> bool {
    let (min, max) = bounds;
    let corners = [
        Point2::new(min.x, min.y),
        Point2::new(min.x, max.y),
        Point2::new(max.x, min.y),
        Point2::new(max.x, max.y),
    ];

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for corner in corners {
        let screen = viewport.world_to_screen(corner);
        min_x = min_x.min(screen.x);
        min_y = min_y.min(screen.y);
        max_x = max_x.max(screen.x);
        max_y = max_y.max(screen.y);
    }

    !(max_x < clip_rect.min.x
        || min_x > clip_rect.max.x
        || max_y < clip_rect.min.y
        || min_y > clip_rect.max.y)
}

fn decode_cad_text(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            
            // Handle Unicode \U+XXXX
            if next == 'U' && i + 6 < chars.len() && chars[i + 2] == '+' {
                let hex: String = chars[i + 3..i + 7].iter().collect();
                if let Ok(value) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(value) {
                        output.push(c);
                        i += 7;
                        continue;
                    }
                }
            }
            
            // Handle codes with arguments that end in ';'
            // \A, \C, \F, \f, \H, \Q, \S, \T, \W, \p
            if matches!(next, 'A' | 'C' | 'F' | 'f' | 'H' | 'Q' | 'S' | 'T' | 'W' | 'p') {
                let mut j = i + 2;
                let mut found = false;
                let mut stack_text = String::new();
                while j < chars.len() {
                    if chars[j] == ';' {
                        found = true;
                        break;
                    }
                    if next == 'S' {
                        if chars[j] == '^' || chars[j] == '#' {
                            stack_text.push('/');
                        } else {
                            stack_text.push(chars[j]);
                        }
                    }
                    j += 1;
                }
                if found {
                    if next == 'S' {
                        output.push_str(&stack_text);
                    }
                    i = j + 1;
                    continue;
                }
            }
            
            // Handle toggle codes \L, \l, \O, \o
            if matches!(next, 'L' | 'l' | 'O' | 'o') {
                i += 2;
                continue;
            }
            
            // Handle Newline \P
            if next == 'P' {
                output.push('\n');
                i += 2;
                continue;
            }
            
            // Handle non-breaking space \~
            if next == '~' {
                output.push(' ');
                i += 2;
                continue;
            }
        }
        
        // Skip braces
        if ch == '{' || ch == '}' {
            i += 1;
            continue;
        }
        
        output.push(ch);
        i += 1;
    }
    
    output
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use acadrust::{CadDocument, Spline, entities::SplineFlags};

    use crate::{
        extraction::models::{
            BlockInfo, BulgeVertex, CadColorSpec, CadLineWeightSpec, EntityStyle,
            ExtractedDrawing, ExtractionStats, InsertTransform, LayerInfo, Point2,
            Polyline2DGeometry, SceneEntity, SceneGeometry, SplineGeometry,
        },
        render::{painter::ViewportState, transform::Affine2},
    };

    use super::{
        arc_render_segments, build_root_draw_queue, circle_render_segments,
        compute_visible_bounds, flatten_polyline2d_for_render, sample_spline_geometry_for_render,
        transformed_bounds,
    };

    #[test]
    fn computes_bounds_from_inserted_block_geometry() {
        let entities = vec![
            SceneEntity {
                id: 0,
                handle: 1,
                owner_handle: 0,
                entity_type: "INSERT".to_owned(),
                layer_name: "0".to_owned(),
                block_name: Some("*Model_Space".to_owned()),
                style: default_style(),
                geometry: SceneGeometry::Insert {
                    block_name: "A".to_owned(),
                    transform: InsertTransform {
                        position: Point2::new(1_000.0, 2_000.0),
                        scale_x: 1.0,
                        scale_y: 1.0,
                        rotation: 0.0,
                    },
                },
            },
            SceneEntity {
                id: 1,
                handle: 2,
                owner_handle: 0,
                entity_type: "LINE".to_owned(),
                layer_name: "0".to_owned(),
                block_name: Some("A".to_owned()),
                style: default_style(),
                geometry: SceneGeometry::Line {
                    start: Point2::new(0.0, 0.0),
                    end: Point2::new(10.0, 20.0),
                },
            },
        ];

        let drawing = ExtractedDrawing {
            source_path: PathBuf::from("test.dwg"),
            document: CadDocument::new(),
            entities,
            entity_by_handle: BTreeMap::new(),
            layer_index: BTreeMap::new(),
            block_index: BTreeMap::from([("A".to_owned(), vec![1])]),
            layers: BTreeMap::from([(
                "0".to_owned(),
                LayerInfo {
                    name: "0".to_owned(),
                    visible_by_default: true,
                    entity_count: 2,
                    color: CadColorSpec::ByLayer,
                    line_weight: CadLineWeightSpec::ByLayer,
                },
            )]),
            blocks: BTreeMap::from([(
                "A".to_owned(),
                BlockInfo {
                    name: "A".to_owned(),
                    entity_count: 1,
                },
            )]),
            layouts: Vec::new(),
            bounds: None,
            stats: ExtractionStats {
                total_entities: 2,
                renderable_entities: 2,
                ignored_entities: 0,
                load_duration_ms: 0,
            },
        };

        let visible = [0usize, 1usize].into_iter().collect();
        let bounds = compute_visible_bounds(&drawing, &visible, Some("*Model_Space")).unwrap();
        assert_eq!(bounds.min, Point2::new(1_000.0, 2_000.0));
        assert_eq!(bounds.max, Point2::new(1_010.0, 2_020.0));
    }

    #[test]
    fn transforms_bounds_for_primitives() {
        let entity = SceneEntity {
            id: 0,
            handle: 1,
            owner_handle: 0,
            entity_type: "LINE".to_owned(),
            layer_name: "0".to_owned(),
            block_name: None,
            style: default_style(),
            geometry: SceneGeometry::Line {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(5.0, 10.0),
            },
        };

        let bounds = transformed_bounds(&entity, Affine2::from_trs(100.0, 200.0, 1.0, 1.0, 0.0)).unwrap();
        assert_eq!(bounds.0, Point2::new(100.0, 200.0));
        assert_eq!(bounds.1, Point2::new(105.0, 210.0));
    }

    #[test]
    fn computes_tight_bounds_for_rotated_scaled_circle() {
        let entity = SceneEntity {
            id: 0,
            handle: 1,
            owner_handle: 0,
            entity_type: "CIRCLE".to_owned(),
            layer_name: "0".to_owned(),
            block_name: None,
            style: default_style(),
            geometry: SceneGeometry::Circle {
                center: Point2::new(0.0, 0.0),
                radius: 3.0,
            },
        };

        let bounds = transformed_bounds(
            &entity,
            Affine2::from_trs(100.0, 200.0, 2.0, 4.0, std::f64::consts::FRAC_PI_4),
        )
        .unwrap();

        let extent = 3.0 * 10.0_f64.sqrt();
        let epsilon = 1e-6;
        assert!((bounds.0.x - (100.0 - extent)).abs() < epsilon);
        assert!((bounds.0.y - (200.0 - extent)).abs() < epsilon);
        assert!((bounds.1.x - (100.0 + extent)).abs() < epsilon);
        assert!((bounds.1.y - (200.0 + extent)).abs() < epsilon);
    }

    #[test]
    fn treats_uppercase_model_space_as_root() {
        let drawing = ExtractedDrawing {
            source_path: PathBuf::from("test.dwg"),
            document: CadDocument::new(),
            entities: vec![
                SceneEntity {
                    id: 0,
                    handle: 1,
                    owner_handle: 0,
                    entity_type: "LINE".to_owned(),
                    layer_name: "0".to_owned(),
                    block_name: Some("*MODEL_SPACE".to_owned()),
                    style: default_style(),
                    geometry: SceneGeometry::Line {
                        start: Point2::new(0.0, 0.0),
                        end: Point2::new(1.0, 1.0),
                    },
                },
                SceneEntity {
                    id: 1,
                    handle: 2,
                    owner_handle: 0,
                    entity_type: "LINE".to_owned(),
                    layer_name: "0".to_owned(),
                    block_name: Some("Other".to_owned()),
                    style: default_style(),
                    geometry: SceneGeometry::Line {
                        start: Point2::new(0.0, 0.0),
                        end: Point2::new(1.0, 1.0),
                    },
                },
            ],
            entity_by_handle: BTreeMap::new(),
            layer_index: BTreeMap::new(),
            block_index: BTreeMap::new(),
            layers: BTreeMap::new(),
            blocks: BTreeMap::new(),
            layouts: Vec::new(),
            bounds: None,
            stats: ExtractionStats {
                total_entities: 2,
                renderable_entities: 2,
                ignored_entities: 0,
                load_duration_ms: 0,
            },
        };

        let visible = [0usize, 1usize].into_iter().collect();
        assert_eq!(
            build_root_draw_queue(&drawing, &visible, Some("*Model_Space")),
            vec![0]
        );
    }

    #[test]
    fn render_curve_sampling_scales_with_zoom() {
        let low_zoom = ViewportState {
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
        };
        let high_zoom = ViewportState {
            zoom: 200.0,
            pan: egui::Vec2::ZERO,
        };

        let low_arc_segments = arc_render_segments(
            &low_zoom,
            Affine2::IDENTITY,
            10.0,
            0.0,
            std::f64::consts::PI,
        );
        let high_arc_segments = arc_render_segments(
            &high_zoom,
            Affine2::IDENTITY,
            10.0,
            0.0,
            std::f64::consts::PI,
        );

        assert!(high_arc_segments > low_arc_segments);
        assert!(circle_render_segments(&high_zoom, Affine2::IDENTITY, 10.0) > 100);
    }

    #[test]
    fn bulged_polyline_sampling_scales_with_zoom() {
        let polyline = Polyline2DGeometry {
            vertices: vec![
                BulgeVertex {
                    location: Point2::new(0.0, 0.0),
                    bulge: 1.0,
                },
                BulgeVertex {
                    location: Point2::new(20.0, 0.0),
                    bulge: 0.0,
                },
            ],
            closed: false,
        };
        let low_zoom = ViewportState {
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
        };
        let high_zoom = ViewportState {
            zoom: 200.0,
            pan: egui::Vec2::ZERO,
        };

        let low_points = flatten_polyline2d_for_render(&low_zoom, Affine2::IDENTITY, &polyline);
        let high_points = flatten_polyline2d_for_render(&high_zoom, Affine2::IDENTITY, &polyline);

        assert!(high_points.len() > low_points.len());
        assert!(high_points.len() > 100);
    }

    #[test]
    fn spline_sampling_scales_with_zoom() {
        let control_points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 20.0),
            Point2::new(20.0, -20.0),
            Point2::new(30.0, 0.0),
        ];
        let spline = SplineGeometry {
            degree: 3,
            flags: SplineFlags::default(),
            knots: Spline::generate_clamped_knots(3, control_points.len()),
            control_points,
            weights: Vec::new(),
            fit_points: Vec::new(),
        };
        let low_zoom = ViewportState {
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
        };
        let high_zoom = ViewportState {
            zoom: 200.0,
            pan: egui::Vec2::ZERO,
        };

        let low_points = sample_spline_geometry_for_render(&low_zoom, Affine2::IDENTITY, &spline);
        let high_points =
            sample_spline_geometry_for_render(&high_zoom, Affine2::IDENTITY, &spline);

        assert!(high_points.len() > low_points.len());
        assert!(high_points.len() > 100);
    }

    fn default_style() -> EntityStyle {
        EntityStyle {
            color: CadColorSpec::ByLayer,
            line_weight: CadLineWeightSpec::ByLayer,
        }
    }
}
