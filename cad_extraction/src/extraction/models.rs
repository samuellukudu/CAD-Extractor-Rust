use std::{collections::BTreeMap, path::PathBuf};

use acadrust::CadDocument;

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
    Polyline {
        points: Vec<Point2>,
        closed: bool,
    },
    Spline {
        control_points: Vec<Point2>,
        fit_points: Vec<Point2>,
    },
    Solid {
        points: Vec<Point2>,
    },
    Hatch {
        loops: Vec<Vec<Point2>>,
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
            SceneGeometry::Polyline { points, .. } => {
                for point in points {
                    visitor(*point);
                }
            }
            SceneGeometry::Spline {
                control_points,
                fit_points,
            } => {
                for point in fit_points {
                    visitor(*point);
                }
                if fit_points.is_empty() {
                    for point in control_points {
                        visitor(*point);
                    }
                }
            }
            SceneGeometry::Solid { points } => {
                for point in points {
                    visitor(*point);
                }
            }
            SceneGeometry::Hatch { loops, .. } => {
                for loop_points in loops {
                    for point in loop_points {
                        visitor(*point);
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
    pub bounds: Option<Bounds2D>,
    pub stats: ExtractionStats,
}
