use std::{collections::BTreeMap, path::PathBuf};

use acadrust::{CadDocument, entities::{SplineFlags, hatch::BoundaryPathFlags}};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds2D {
    pub min: Point2,
    pub max: Point2,
}

impl Bounds2D {
    pub fn from_point(point: Point2) -> Self {
        Self {
            min: point,
            max: point,
        }
    }

    pub fn include_point(&mut self, point: Point2) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadColorSpec {
    ByLayer,
    ByBlock,
    Index(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadLineWeightSpec {
    ByLayer,
    ByBlock,
    Default,
    Value(i16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSource {
    TrueColor,
    Aci,
    Layer,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityStyle {
    pub color: CadColorSpec,
    pub line_weight: CadLineWeightSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextPayload {
    pub value: String,
    pub height: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsertTransform {
    pub position: Point2,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct BulgeVertex {
    pub location: Point2,
    pub bulge: f64,
}

#[derive(Debug, Clone)]
pub struct Polyline2DGeometry {
    pub vertices: Vec<BulgeVertex>,
    pub closed: bool,
}

#[derive(Debug, Clone)]
pub struct Polyline3DGeometry {
    pub vertices: Vec<Point2>,
    pub closed: bool,
}

#[derive(Debug, Clone)]
pub struct SplineGeometry {
    pub degree: i32,
    pub flags: SplineFlags,
    pub knots: Vec<f64>,
    pub control_points: Vec<Point2>,
    pub weights: Vec<f64>,
    pub fit_points: Vec<Point2>,
}

#[derive(Debug, Clone)]
pub enum HatchBoundaryEdgeGeometry {
    Line {
        start: Point2,
        end: Point2,
    },
    CircularArc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        counter_clockwise: bool,
    },
    EllipticArc {
        center: Point2,
        major_axis_endpoint: Point2,
        minor_axis_ratio: f64,
        start_angle: f64,
        end_angle: f64,
        counter_clockwise: bool,
    },
    Spline(SplineGeometry),
    Polyline(Polyline2DGeometry),
}

#[derive(Debug, Clone)]
pub struct HatchBoundaryPathGeometry {
    pub flags: BoundaryPathFlags,
    pub edges: Vec<HatchBoundaryEdgeGeometry>,
    pub boundary_handles: Vec<u64>,
}

#[derive(Debug, Clone)]
pub enum SceneGeometry {
    Line {
        start: Point2,
        end: Point2,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Ellipse {
        center: Point2,
        major_axis: Point2,
        minor_axis_ratio: f64,
        start_parameter: f64,
        end_parameter: f64,
    },
    LwPolyline {
        polyline: Polyline2DGeometry,
    },
    Spline {
        spline: SplineGeometry,
    },
    Polyline2D {
        polyline: Polyline2DGeometry,
    },
    Polyline3D {
        polyline: Polyline3DGeometry,
    },
    Solid {
        points: Vec<Point2>,
    },
    Hatch {
        paths: Vec<HatchBoundaryPathGeometry>,
        solid_fill: bool,
    },
    Text {
        position: Point2,
        payload: TextPayload,
    },
    Insert {
        block_name: String,
        transform: InsertTransform,
    },
    Dimension {
        block_name: String,
        transform: InsertTransform,
    },
    Unsupported {
        reason: String,
    },
}

impl SceneGeometry {
    pub fn visit_points<F: FnMut(Point2)>(&self, mut visitor: F) {
        match self {
            SceneGeometry::Line { start, end } => {
                visitor(*start);
                visitor(*end);
            }
            SceneGeometry::Circle { center, radius } => {
                let r = radius.abs();
                visitor(Point2::new(center.x - r, center.y - r));
                visitor(Point2::new(center.x + r, center.y + r));
            }
            SceneGeometry::Arc { center, radius, .. } => {
                let r = radius.abs();
                visitor(Point2::new(center.x - r, center.y - r));
                visitor(Point2::new(center.x + r, center.y + r));
            }
            SceneGeometry::Ellipse {
                center, major_axis, ..
            } => {
                let radius = (major_axis.x * major_axis.x + major_axis.y * major_axis.y).sqrt();
                visitor(Point2::new(center.x - radius, center.y - radius));
                visitor(Point2::new(center.x + radius, center.y + radius));
            }
            SceneGeometry::LwPolyline { polyline } | SceneGeometry::Polyline2D { polyline } => {
                for vertex in &polyline.vertices {
                    visitor(vertex.location);
                }
            }
            SceneGeometry::Polyline3D { polyline } => {
                for point in &polyline.vertices {
                    visitor(*point);
                }
            }
            SceneGeometry::Spline { spline } => {
                for point in &spline.control_points {
                    visitor(*point);
                }
                if spline.control_points.is_empty() {
                    for point in &spline.fit_points {
                        visitor(*point);
                    }
                }
            }
            SceneGeometry::Solid { points } => {
                for point in points {
                    visitor(*point);
                }
            }
            SceneGeometry::Hatch { paths, .. } => {
                for path in paths {
                    for edge in &path.edges {
                        match edge {
                            HatchBoundaryEdgeGeometry::Line { start, end } => {
                                visitor(*start);
                                visitor(*end);
                            }
                            HatchBoundaryEdgeGeometry::CircularArc { center, radius, .. } => {
                                let r = radius.abs();
                                visitor(Point2::new(center.x - r, center.y - r));
                                visitor(Point2::new(center.x + r, center.y + r));
                            }
                            HatchBoundaryEdgeGeometry::EllipticArc {
                                center,
                                major_axis_endpoint,
                                ..
                            } => {
                                let radius = (major_axis_endpoint.x * major_axis_endpoint.x
                                    + major_axis_endpoint.y * major_axis_endpoint.y)
                                    .sqrt();
                                visitor(Point2::new(center.x - radius, center.y - radius));
                                visitor(Point2::new(center.x + radius, center.y + radius));
                            }
                            HatchBoundaryEdgeGeometry::Spline(spline) => {
                                for point in &spline.control_points {
                                    visitor(*point);
                                }
                                if spline.control_points.is_empty() {
                                    for point in &spline.fit_points {
                                        visitor(*point);
                                    }
                                }
                            }
                            HatchBoundaryEdgeGeometry::Polyline(polyline) => {
                                for vertex in &polyline.vertices {
                                    visitor(vertex.location);
                                }
                            }
                        }
                    }
                }
            }
            SceneGeometry::Text { position, .. } => visitor(*position),
            SceneGeometry::Insert { transform, .. } => visitor(transform.position),
            SceneGeometry::Dimension { transform, .. } => visitor(transform.position),
            SceneGeometry::Unsupported { .. } => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct SceneEntity {
    pub id: usize,
    pub handle: u64,
    pub owner_handle: u64,
    pub entity_type: String,
    pub layer_name: String,
    pub block_name: Option<String>,
    pub style: EntityStyle,
    pub geometry: SceneGeometry,
}

#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub name: String,
    pub visible_by_default: bool,
    pub entity_count: usize,
    pub color: CadColorSpec,
    pub line_weight: CadLineWeightSpec,
}

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub name: String,
    pub entity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutInfo {
    pub name: String,
    pub root_block_name: String,
    pub tab_order: i16,
    pub is_model: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractionStats {
    pub total_entities: usize,
    pub renderable_entities: usize,
    pub ignored_entities: usize,
    pub load_duration_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ExtractedDrawing {
    pub source_path: PathBuf,
    pub document: CadDocument,
    pub entities: Vec<SceneEntity>,
    pub entity_by_handle: BTreeMap<u64, usize>,
    pub layer_index: BTreeMap<String, Vec<usize>>,
    pub block_index: BTreeMap<String, Vec<usize>>,
    pub layers: BTreeMap<String, LayerInfo>,
    pub blocks: BTreeMap<String, BlockInfo>,
    pub layouts: Vec<LayoutInfo>,
    pub bounds: Option<Bounds2D>,
    pub stats: ExtractionStats,
}
