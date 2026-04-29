use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

use acadrust::{
    CadDocument, Color, EntityType, Handle, LineWeight,
    entities::{SplineFlags, hatch::{BoundaryEdge, BoundaryPath}},
    objects::ObjectType,
};

use super::{
    error::ExtractionError,
    models::{
        BlockInfo, Bounds2D, BulgeVertex, CadColorSpec, CadLineWeightSpec, EntityStyle,
        ExtractedDrawing, ExtractionStats, HatchBoundaryEdgeGeometry, HatchBoundaryPathGeometry,
        InsertTransform, LayerInfo, LayoutInfo, Point2, Polyline2DGeometry, Polyline3DGeometry,
        SceneEntity, SceneGeometry, SplineGeometry, TextPayload,
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

    let block_name_by_handle: BTreeMap<Handle, String> = document
        .block_records
        .iter()
        .map(|record| (record.handle, record.name.clone()))
        .collect();

    let mut layouts: Vec<LayoutInfo> = document
        .objects
        .values()
        .filter_map(|object| {
            let ObjectType::Layout(layout) = object else {
                return None;
            };
            let root_block_name = block_name_by_handle.get(&layout.block_record)?.clone();
            Some(LayoutInfo {
                is_model: root_block_name.eq_ignore_ascii_case("*Model_Space"),
                name: layout.name.clone(),
                root_block_name,
                tab_order: layout.tab_order,
            })
        })
        .collect();

    layouts.sort_by_key(|layout| (!layout.is_model, layout.tab_order, layout.name.clone()));

    if layouts.is_empty() {
        layouts.push(LayoutInfo {
            name: "Model".to_owned(),
            root_block_name: "*Model_Space".to_owned(),
            tab_order: 0,
            is_model: true,
        });
    }

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
        layouts,
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
        EntityType::LwPolyline(polyline) => SceneGeometry::LwPolyline {
            polyline: Polyline2DGeometry {
                vertices: polyline
                    .vertices
                    .iter()
                    .map(|vertex| BulgeVertex {
                        location: Point2::new(vertex.location.x, vertex.location.y),
                        bulge: vertex.bulge,
                    })
                    .collect(),
                closed: polyline.is_closed,
            },
        },
        EntityType::Polyline(polyline) => SceneGeometry::Polyline3D {
            polyline: Polyline3DGeometry {
                vertices: polyline
                    .vertices
                    .iter()
                    .map(|vertex| Point2::new(vertex.location.x, vertex.location.y))
                    .collect(),
                closed: polyline.is_closed(),
            },
        },
        EntityType::Polyline2D(polyline) => SceneGeometry::Polyline2D {
            polyline: Polyline2DGeometry {
                vertices: polyline
                    .vertices
                    .iter()
                    .map(|vertex| BulgeVertex {
                        location: Point2::new(vertex.location.x, vertex.location.y),
                        bulge: vertex.bulge,
                    })
                    .collect(),
                closed: polyline.is_closed(),
            },
        },
        EntityType::Spline(spline) => SceneGeometry::Spline {
            spline: SplineGeometry {
                degree: spline.degree,
                flags: spline.flags,
                knots: spline.knots.clone(),
                control_points: spline
                    .control_points
                    .iter()
                    .map(|point| Point2::new(point.x, point.y))
                    .collect(),
                weights: spline.weights.clone(),
                fit_points: spline
                    .fit_points
                    .iter()
                    .map(|point| Point2::new(point.x, point.y))
                    .collect(),
            },
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
            paths: hatch.paths.iter().map(to_hatch_boundary_path).collect(),
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

fn to_hatch_boundary_path(path: &BoundaryPath) -> HatchBoundaryPathGeometry {
    HatchBoundaryPathGeometry {
        flags: path.flags,
        edges: path.edges.iter().map(to_hatch_boundary_edge).collect(),
        boundary_handles: path
            .boundary_handles
            .iter()
            .map(|handle| handle.value())
            .collect(),
    }
}

fn to_hatch_boundary_edge(edge: &BoundaryEdge) -> HatchBoundaryEdgeGeometry {
    match edge {
        BoundaryEdge::Line(line) => HatchBoundaryEdgeGeometry::Line {
            start: Point2::new(line.start.x, line.start.y),
            end: Point2::new(line.end.x, line.end.y),
        },
        BoundaryEdge::CircularArc(arc) => HatchBoundaryEdgeGeometry::CircularArc {
            center: Point2::new(arc.center.x, arc.center.y),
            radius: arc.radius,
            start_angle: arc.start_angle,
            end_angle: arc.end_angle,
            counter_clockwise: arc.counter_clockwise,
        },
        BoundaryEdge::EllipticArc(ellipse) => HatchBoundaryEdgeGeometry::EllipticArc {
            center: Point2::new(ellipse.center.x, ellipse.center.y),
            major_axis_endpoint: Point2::new(
                ellipse.major_axis_endpoint.x,
                ellipse.major_axis_endpoint.y,
            ),
            minor_axis_ratio: ellipse.minor_axis_ratio,
            start_angle: ellipse.start_angle,
            end_angle: ellipse.end_angle,
            counter_clockwise: ellipse.counter_clockwise,
        },
        BoundaryEdge::Spline(spline) => HatchBoundaryEdgeGeometry::Spline(SplineGeometry {
            degree: spline.degree,
            flags: SplineFlags {
                closed: false,
                periodic: spline.periodic,
                rational: spline.rational,
                planar: true,
                linear: spline.degree <= 1,
            },
            knots: spline.knots.clone(),
            control_points: spline
                .control_points
                .iter()
                .map(|point| Point2::new(point.x, point.y))
                .collect(),
            weights: spline
                .control_points
                .iter()
                .map(|point| if spline.rational { point.z } else { 1.0 })
                .collect(),
            fit_points: spline
                .fit_points
                .iter()
                .map(|point| Point2::new(point.x, point.y))
                .collect(),
        }),
        BoundaryEdge::Polyline(polyline) => HatchBoundaryEdgeGeometry::Polyline(Polyline2DGeometry {
            vertices: polyline
                .vertices
                .iter()
                .map(|point| BulgeVertex {
                    location: Point2::new(point.x, point.y),
                    bulge: point.z,
                })
                .collect(),
            closed: polyline.is_closed,
        }),
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

#[cfg(test)]
mod tests {
    use acadrust::{
        EntityType, LwPolyline, Spline, Vector2, Vector3,
        entities::{
            hatch::{BoundaryEdge, BoundaryPath, SplineEdge},
            lwpolyline::LwVertex,
        },
    };

    use crate::extraction::models::{HatchBoundaryEdgeGeometry, SceneGeometry};

    use super::to_scene_geometry;

    #[test]
    fn preserves_lwpolyline_bulge_vertices() {
        let polyline = LwPolyline {
            vertices: vec![
                LwVertex::with_bulge(Vector2::new(0.0, 0.0), 1.0),
                LwVertex::new(Vector2::new(10.0, 0.0)),
            ],
            ..LwPolyline::new()
        };

        let geometry = to_scene_geometry(&EntityType::LwPolyline(polyline));

        match geometry {
            SceneGeometry::LwPolyline { polyline } => {
                assert_eq!(polyline.vertices.len(), 2);
                assert_eq!(polyline.vertices[0].location.x, 0.0);
                assert_eq!(polyline.vertices[0].bulge, 1.0);
                assert_eq!(polyline.vertices[1].location.x, 10.0);
            }
            other => panic!("expected lwpolyline geometry, got {other:?}"),
        }
    }

    #[test]
    fn preserves_spline_definition() {
        let spline = Spline::from_control_points(
            2,
            vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(5.0, 10.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
            ],
        );

        let geometry = to_scene_geometry(&EntityType::Spline(spline));

        match geometry {
            SceneGeometry::Spline { spline } => {
                assert_eq!(spline.degree, 2);
                assert_eq!(spline.control_points.len(), 3);
                assert!(spline.knots.len() >= 6);
                assert!(spline.fit_points.is_empty());
            }
            other => panic!("expected spline geometry, got {other:?}"),
        }
    }

    #[test]
    fn preserves_hatch_spline_edges() {
        let mut path = BoundaryPath::new();
        path.add_edge(BoundaryEdge::Spline(SplineEdge {
            degree: 2,
            rational: false,
            periodic: false,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control_points: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(5.0, 10.0, 0.0),
                Vector3::new(10.0, 0.0, 0.0),
            ],
            fit_points: Vec::new(),
            start_tangent: Vector2::new(0.0, 0.0),
            end_tangent: Vector2::new(0.0, 0.0),
        }));

        let converted = super::to_hatch_boundary_path(&path);

        assert_eq!(converted.edges.len(), 1);
        match &converted.edges[0] {
            HatchBoundaryEdgeGeometry::Spline(spline) => {
                assert_eq!(spline.degree, 2);
                assert_eq!(spline.control_points.len(), 3);
                assert!(spline.fit_points.is_empty());
            }
            other => panic!("expected spline hatch edge, got {other:?}"),
        }
    }
}
