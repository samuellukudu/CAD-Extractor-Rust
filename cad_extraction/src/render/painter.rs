use std::collections::BTreeSet;

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::extraction::models::{Bounds2D, ExtractedDrawing, Point2, SceneEntity, SceneGeometry};

use super::{
    style::{ResolvedStyle, resolve_style},
    transform::Affine2,
};

const ARC_SEGMENTS: usize = 64;
const MAX_RECURSION_DEPTH: usize = 30;

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

pub fn draw_scene(
    ui: &mut Ui,
    viewport: &mut ViewportState,
    drawing: &ExtractedDrawing,
    visible_entity_ids: &BTreeSet<usize>,
    selected_entity: Option<usize>,
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
    let root_ids = build_root_draw_queue(drawing, visible_entity_ids);
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
) -> Option<usize> {
    let mut closest: Option<(usize, f32)> = None;
    let roots = build_root_draw_queue(drawing, visible_entity_ids);
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

fn build_root_draw_queue(drawing: &ExtractedDrawing, visible_entity_ids: &BTreeSet<usize>) -> Vec<usize> {
    let mut roots: Vec<usize> = drawing
        .entities
        .iter()
        .filter(|entity| {
            visible_entity_ids.contains(&entity.id)
                && matches!(entity.block_name.as_deref(), Some("*Model_Space") | None)
        })
        .map(|entity| entity.id)
        .collect();

    roots.sort_by_key(|id| {
        let is_hatch = matches!(drawing.entities[*id].geometry, SceneGeometry::Hatch { .. });
        (is_hatch as u8, *id)
    });
    roots
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
            let transformed_center = transform.transform_point(*center);
            let center_screen = viewport.world_to_screen(transformed_center);
            let radius_px = (*radius as f32 * viewport.zoom * transform.scale_hint() as f32)
                .abs()
                .max(1.0);
            painter.circle_stroke(center_screen, radius_px, stroke);
        }
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let mut points = Vec::with_capacity(ARC_SEGMENTS + 1);
            for i in 0..=ARC_SEGMENTS {
                let t = i as f64 / ARC_SEGMENTS as f64;
                let angle = start_angle + (end_angle - start_angle) * t;
                let point = Point2::new(
                    center.x + angle.cos() * radius,
                    center.y + angle.sin() * radius,
                );
                points.push(viewport.world_to_screen(transform.transform_point(point)));
            }
            painter.add(egui::Shape::line(points, stroke));
        }
        SceneGeometry::Ellipse {
            center,
            major_axis,
            minor_axis_ratio,
            start_parameter,
            end_parameter,
        } => {
            let mut points = Vec::with_capacity(ARC_SEGMENTS + 1);
            let major_len = (major_axis.x * major_axis.x + major_axis.y * major_axis.y).sqrt().max(1e-6);
            let minor_len = major_len * *minor_axis_ratio;
            let axis_angle = major_axis.y.atan2(major_axis.x);
            for i in 0..=ARC_SEGMENTS {
                let t = i as f64 / ARC_SEGMENTS as f64;
                let theta = start_parameter + (end_parameter - start_parameter) * t;
                let x = major_len * theta.cos();
                let y = minor_len * theta.sin();
                let rotated = Point2::new(
                    center.x + x * axis_angle.cos() - y * axis_angle.sin(),
                    center.y + x * axis_angle.sin() + y * axis_angle.cos(),
                );
                points.push(viewport.world_to_screen(transform.transform_point(rotated)));
            }
            painter.add(egui::Shape::line(points, stroke));
        }
        SceneGeometry::Polyline { points, closed } => {
            if points.len() >= 2 {
                let mut path: Vec<Pos2> = points
                    .iter()
                    .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
                    .collect();
                if *closed {
                    path.push(viewport.world_to_screen(transform.transform_point(points[0])));
                }
                painter.add(egui::Shape::line(path, stroke));
            }
        }
        SceneGeometry::Spline {
            control_points,
            fit_points,
        } => {
            let points = if fit_points.len() >= 2 {
                fit_points
            } else {
                control_points
            };
            if points.len() >= 2 {
                let mut sampled = Vec::new();
                let interpolation_steps = 4;
                for window in points.windows(2) {
                    let a = window[0];
                    let b = window[1];
                    for i in 0..interpolation_steps {
                        let t = i as f64 / interpolation_steps as f64;
                        let point = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                        sampled.push(viewport.world_to_screen(transform.transform_point(point)));
                    }
                }
                let last = points[points.len() - 1];
                sampled.push(viewport.world_to_screen(transform.transform_point(last)));
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
        SceneGeometry::Hatch { loops, solid_fill } => {
            for loop_points in loops {
                if loop_points.len() < 3 {
                    continue;
                }
                let transformed: Vec<Pos2> = loop_points
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
            let center_px = viewport.world_to_screen(transform.transform_point(*center));
            let radius_px = (*radius as f32) * viewport.zoom * transform.scale_hint() as f32;
            (click_pos.distance(center_px) - radius_px).abs()
        }
        SceneGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let center_px = viewport.world_to_screen(transform.transform_point(*center));
            let radius_px = (*radius as f32) * viewport.zoom * transform.scale_hint() as f32;
            let vector = click_pos - center_px;
            let angle = (vector.y as f64).atan2(vector.x as f64);
            let in_arc = angle_between(angle, *start_angle, *end_angle);
            if in_arc {
                (vector.length() - radius_px).abs()
            } else {
                vector.length().abs()
            }
        }
        SceneGeometry::Polyline { points, closed } => {
            if points.len() < 2 {
                return f32::MAX;
            }
            let mut best = f32::MAX;
            for segment in points.windows(2) {
                let a = viewport.world_to_screen(transform.transform_point(segment[0]));
                let b = viewport.world_to_screen(transform.transform_point(segment[1]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            if *closed {
                let a =
                    viewport.world_to_screen(transform.transform_point(points[points.len() - 1]));
                let b = viewport.world_to_screen(transform.transform_point(points[0]));
                best = best.min(distance_to_segment(click_pos, a, b));
            }
            best
        }
        SceneGeometry::Ellipse {
            center, major_axis, ..
        } => {
            let marker = viewport.world_to_screen(transform.transform_point(*center));
            let radius = (major_axis.x * major_axis.x + major_axis.y * major_axis.y).sqrt();
            (marker.distance(click_pos) - (radius as f32 * viewport.zoom)).abs()
        }
        SceneGeometry::Spline {
            control_points,
            fit_points,
        } => {
            let points = if fit_points.len() >= 2 {
                fit_points
            } else {
                control_points
            };
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
        SceneGeometry::Hatch { loops, .. } => loops
            .iter()
            .flatten()
            .map(|point| viewport.world_to_screen(transform.transform_point(*point)))
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

fn angle_between(value: f64, start: f64, end: f64) -> bool {
    if start <= end {
        value >= start && value <= end
    } else {
        value >= start || value <= end
    }
}

fn transformed_bounds(entity: &SceneEntity, transform: Affine2) -> Option<(Point2, Point2)> {
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
