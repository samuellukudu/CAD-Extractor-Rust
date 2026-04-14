use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

use acadrust::{
    CadDocument, Color, EntityType, Handle, LineWeight,
    entities::hatch::{BoundaryEdge, BoundaryPath},
};

use super::{
    error::ExtractionError,
    models::{
        BlockInfo, Bounds2D, CadColorSpec, CadLineWeightSpec, EntityStyle, ExtractedDrawing,
        ExtractionStats, InsertTransform, LayerInfo, Point2, SceneEntity, SceneGeometry,
        TextPayload,
    },
    reader::read_document,
};

pub fn extract_file(path: &Path) -> Result<ExtractedDrawing, ExtractionError> {
    let start = Instant::now();
    let document = read_document(path)?;
    Ok(extract_document(document, path.to_path_buf(), start.elapsed().as_millis()))
}

pub fn extract_document(
    document: CadDocument,
    source_path: PathBuf,
    load_duration_ms: u128,
) -> ExtractedDrawing {
    let mut layer_index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut block_index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut entity_by_handle: BTreeMap<u64, usize> = BTreeMap::new();
    let mut bounds: Option<Bounds2D> = None;

    let block_by_handle: BTreeMap<Handle, String> = document
        .block_records
        .iter()
        .map(|record| (record.handle, record.name.clone()))
        .collect();

    let mut entities = Vec::with_capacity(document.entity_count());
    let mut ignored_entities = 0usize;

    for (id, entity) in document.entities().enumerate() {
        let common = entity.common();
        let layer_name = common.layer.clone();
        let block_name = block_by_handle.get(&common.owner_handle).cloned();
        let geometry = to_scene_geometry(entity);

        if matches!(geometry, SceneGeometry::Unsupported { .. }) {
            ignored_entities += 1;
        }

        geometry.visit_points(|point| {
            if let Some(active_bounds) = bounds.as_mut() {
                active_bounds.include_point(point);
            } else {
                bounds = Some(Bounds2D::from_point(point));
            }
        });

        let scene_entity = SceneEntity {
            id,
            handle: common.handle.value(),
            owner_handle: common.owner_handle.value(),
            entity_type: entity.as_entity().entity_type().to_owned(),
            layer_name: layer_name.clone(),
            block_name: block_name.clone(),
            style: EntityStyle {
                color: to_color_spec(common.color),
                line_weight: to_line_weight_spec(common.line_weight),
            },
            geometry,
        };

        if common.handle.value() != 0 {
            entity_by_handle.insert(common.handle.value(), id);
        }
        layer_index.entry(layer_name).or_default().push(id);
        if let Some(block_name) = block_name {
            block_index.entry(block_name).or_default().push(id);
        }
        entities.push(scene_entity);
    }

    let mut layers: BTreeMap<String, LayerInfo> = document
        .layers
        .iter()
        .map(|layer| {
            (
                layer.name.clone(),
                LayerInfo {
                    name: layer.name.clone(),
                    visible_by_default: layer.is_visible(),
                    entity_count: 0,
                    color: to_color_spec(layer.color),
                    line_weight: to_line_weight_spec(layer.line_weight),
                },
            )
        })
        .collect();

    for (name, indices) in &layer_index {
        let entry = layers.entry(name.clone()).or_insert(LayerInfo {
            name: name.clone(),
            visible_by_default: true,
            entity_count: 0,
            color: CadColorSpec::ByLayer,
            line_weight: CadLineWeightSpec::ByLayer,
        });
        entry.entity_count = indices.len();
    }

    let blocks: BTreeMap<String, BlockInfo> = block_index
        .iter()
        .map(|(name, indices)| {
            (
                name.clone(),
                BlockInfo {
                    name: name.clone(),
                    entity_count: indices.len(),
                },
            )
        })
        .collect();

    let total_entities = entities.len();
    let renderable_entities = total_entities.saturating_sub(ignored_entities);

    ExtractedDrawing {
        source_path,
        document,
        entities,
        entity_by_handle,
        layer_index,
        block_index,
        layers,
        blocks,
        bounds,
        stats: ExtractionStats {
            total_entities,
            renderable_entities,
            ignored_entities,
            load_duration_ms,
        },
    }
}

fn to_scene_geometry(entity: &EntityType) -> SceneGeometry {
    match entity {
        EntityType::Line(line) => SceneGeometry::Line {
            start: Point2::new(line.start.x, line.start.y),
            end: Point2::new(line.end.x, line.end.y),
        },
        EntityType::Circle(circle) => SceneGeometry::Circle {
            center: Point2::new(circle.center.x, circle.center.y),
            radius: circle.radius,
        },
        EntityType::Arc(arc) => SceneGeometry::Arc {
            center: Point2::new(arc.center.x, arc.center.y),
            radius: arc.radius,
            start_angle: arc.start_angle,
            end_angle: arc.end_angle,
        },
        EntityType::Ellipse(ellipse) => SceneGeometry::Ellipse {
            center: Point2::new(ellipse.center.x, ellipse.center.y),
            major_axis: Point2::new(ellipse.major_axis.x, ellipse.major_axis.y),
            minor_axis_ratio: ellipse.minor_axis_ratio,
            start_parameter: ellipse.start_parameter,
            end_parameter: ellipse.end_parameter,
        },
        EntityType::LwPolyline(polyline) => SceneGeometry::Polyline {
            points: polyline
                .vertices
                .iter()
                .map(|vertex| Point2::new(vertex.location.x, vertex.location.y))
                .collect(),
            closed: polyline.is_closed,
        },
        EntityType::Polyline(polyline) => SceneGeometry::Polyline {
            points: polyline
                .vertices
                .iter()
                .map(|vertex| Point2::new(vertex.location.x, vertex.location.y))
                .collect(),
            closed: polyline.is_closed(),
        },
        EntityType::Polyline2D(polyline) => SceneGeometry::Polyline {
            points: polyline
                .vertices
                .iter()
                .map(|vertex| Point2::new(vertex.location.x, vertex.location.y))
                .collect(),
            closed: polyline.is_closed(),
        },
        EntityType::Spline(spline) => SceneGeometry::Spline {
            control_points: spline
                .control_points
                .iter()
                .map(|point| Point2::new(point.x, point.y))
                .collect(),
            fit_points: spline
                .fit_points
                .iter()
                .map(|point| Point2::new(point.x, point.y))
                .collect(),
        },
        EntityType::Solid(solid) => SceneGeometry::Solid {
            points: vec![
                Point2::new(solid.first_corner.x, solid.first_corner.y),
                Point2::new(solid.second_corner.x, solid.second_corner.y),
                Point2::new(solid.third_corner.x, solid.third_corner.y),
                Point2::new(solid.fourth_corner.x, solid.fourth_corner.y),
            ],
        },
        EntityType::Hatch(hatch) => SceneGeometry::Hatch {
            loops: hatch.paths.iter().map(path_to_points).collect(),
            solid_fill: hatch.is_solid,
        },
        EntityType::Text(text) => SceneGeometry::Text {
            position: Point2::new(text.insertion_point.x, text.insertion_point.y),
            payload: TextPayload {
                value: text.value.clone(),
                height: text.height,
                rotation: text.rotation,
            },
        },
        EntityType::MText(text) => SceneGeometry::Text {
            position: Point2::new(text.insertion_point.x, text.insertion_point.y),
            payload: TextPayload {
                value: text.value.clone(),
                height: text.height,
                rotation: text.rotation,
            },
        },
        EntityType::Insert(insert) => SceneGeometry::Insert {
            block_name: insert.block_name.clone(),
            transform: InsertTransform {
                position: Point2::new(insert.insert_point.x, insert.insert_point.y),
                scale_x: insert.x_scale(),
                scale_y: insert.y_scale(),
                rotation: insert.rotation,
            },
        },
        EntityType::Dimension(dimension) => SceneGeometry::Dimension {
            block_name: dimension.base().block_name.clone(),
            transform: InsertTransform {
                position: Point2::new(
                    dimension.base().insertion_point.x,
                    dimension.base().insertion_point.y,
                ),
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: dimension.base().text_rotation,
            },
        },
        _ => SceneGeometry::Unsupported {
            reason: "not yet supported by scene extractor".to_owned(),
        },
    }
}

fn to_color_spec(color: Color) -> CadColorSpec {
    match color {
        Color::ByLayer => CadColorSpec::ByLayer,
        Color::ByBlock => CadColorSpec::ByBlock,
        Color::Index(index) => CadColorSpec::Index(index),
        Color::Rgb { r, g, b } => CadColorSpec::Rgb(r, g, b),
    }
}

fn to_line_weight_spec(line_weight: LineWeight) -> CadLineWeightSpec {
    match line_weight {
        LineWeight::ByLayer => CadLineWeightSpec::ByLayer,
        LineWeight::ByBlock => CadLineWeightSpec::ByBlock,
        LineWeight::Default => CadLineWeightSpec::Default,
        LineWeight::Value(value) => CadLineWeightSpec::Value(value),
    }
}

fn path_to_points(path: &BoundaryPath) -> Vec<Point2> {
    let mut points = Vec::new();
    for edge in &path.edges {
        match edge {
            BoundaryEdge::Line(line) => {
                points.push(Point2::new(line.start.x, line.start.y));
                points.push(Point2::new(line.end.x, line.end.y));
            }
            BoundaryEdge::CircularArc(arc) => {
                let steps = 24usize;
                let mut start = arc.start_angle;
                let mut end = arc.end_angle;
                if arc.counter_clockwise {
                    while end < start {
                        end += std::f64::consts::TAU;
                    }
                } else {
                    while start < end {
                        start += std::f64::consts::TAU;
                    }
                }
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let angle = if arc.counter_clockwise {
                        start + (end - start) * t
                    } else {
                        start - (start - end) * t
                    };
                    points.push(Point2::new(
                        arc.center.x + arc.radius * angle.cos(),
                        arc.center.y + arc.radius * angle.sin(),
                    ));
                }
            }
            BoundaryEdge::EllipticArc(ellipse) => {
                let steps = 36usize;
                let major = ellipse.major_axis_endpoint;
                let major_len = (major.x * major.x + major.y * major.y).sqrt().max(1e-6);
                let minor_len = major_len * ellipse.minor_axis_ratio;
                let axis_angle = major.y.atan2(major.x);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let angle = ellipse.start_angle + (ellipse.end_angle - ellipse.start_angle) * t;
                    let x = major_len * angle.cos();
                    let y = minor_len * angle.sin();
                    let rotated_x = x * axis_angle.cos() - y * axis_angle.sin();
                    let rotated_y = x * axis_angle.sin() + y * axis_angle.cos();
                    points.push(Point2::new(
                        ellipse.center.x + rotated_x,
                        ellipse.center.y + rotated_y,
                    ));
                }
            }
            BoundaryEdge::Spline(spline) => {
                for point in &spline.fit_points {
                    points.push(Point2::new(point.x, point.y));
                }
                if spline.fit_points.is_empty() {
                    for point in &spline.control_points {
                        points.push(Point2::new(point.x, point.y));
                    }
                }
            }
            BoundaryEdge::Polyline(polyline) => {
                for point in &polyline.vertices {
                    points.push(Point2::new(point.x, point.y));
                }
            }
        }
    }
    points
}
