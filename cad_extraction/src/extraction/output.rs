//! Output serialization types for CAD extraction JSON.
//!
//! These types define the JSON schema that matches (and improves upon) the
//! reference JavaScript DXF parser output format.  The internal extraction
//! models (`SceneEntity`, `ExtractedDrawing`, …) are converted into these
//! output types before serialization.

use std::collections::BTreeMap;

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

// ────────────────────────── Top-level output ──────────────────────────

/// Top-level JSON output for a CAD file extraction.
#[derive(Debug, Serialize)]
pub struct CadOutput {
    pub header: HeaderOutput,
    pub tables: TablesOutput,
    pub blocks: BTreeMap<String, BlockOutput>,
}

// ────────────────────────── Header ──────────────────────────

/// Header variables serialized as a flat `$VAR_NAME -> value` map.
#[derive(Debug)]
pub struct HeaderOutput {
    pub vars: BTreeMap<String, HeaderValue>,
}

impl Serialize for HeaderOutput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.vars.len()))?;
        for (key, value) in &self.vars {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// A header variable value – can be a string, number, bool, point, or null.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum HeaderValue {
    String(String),
    Float(f64),
    Int(i64),
    Bool(bool),
    Point2D(Point2DOutput),
    Point3D(Point3DOutput),
}

// ────────────────────────── Geometry primitives ──────────────────────────

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Point2DOutput {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Point3DOutput {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

// ────────────────────────── Tables ──────────────────────────

#[derive(Debug, Serialize)]
pub struct TablesOutput {
    #[serde(rename = "viewPort")]
    pub viewport: ViewPortTableOutput,
    #[serde(rename = "lineType")]
    pub line_type: LineTypeTableOutput,
    pub layer: LayerTableOutput,
    pub style: StyleTableOutput,
    pub dimstyle: DimStyleTableOutput,
}

// ── Viewport table ──

#[derive(Debug, Serialize)]
pub struct ViewPortTableOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    #[serde(rename = "viewPorts")]
    pub viewports: Vec<ViewPortEntryOutput>,
}

#[derive(Debug, Serialize)]
pub struct ViewPortEntryOutput {
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    pub name: String,
    #[serde(rename = "lowerLeftCorner")]
    pub lower_left_corner: Point2DOutput,
    #[serde(rename = "upperRightCorner")]
    pub upper_right_corner: Point2DOutput,
    pub center: Point2DOutput,
    #[serde(rename = "snapBasePoint")]
    pub snap_base_point: Point2DOutput,
    #[serde(rename = "snapSpacing")]
    pub snap_spacing: Point2DOutput,
    #[serde(rename = "gridSpacing")]
    pub grid_spacing: Point2DOutput,
    #[serde(rename = "viewDirectionFromTarget")]
    pub view_direction_from_target: Point3DOutput,
    #[serde(rename = "viewTarget")]
    pub view_target: Point3DOutput,
    #[serde(rename = "viewTwistAngle")]
    pub view_twist_angle: f64,
    #[serde(rename = "renderMode")]
    pub render_mode: i32,
}

// ── Line type table ──

#[derive(Debug, Serialize)]
pub struct LineTypeTableOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    #[serde(rename = "lineTypes")]
    pub line_types: BTreeMap<String, LineTypeEntryOutput>,
}

#[derive(Debug, Serialize)]
pub struct LineTypeEntryOutput {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pattern: Vec<f64>,
    #[serde(rename = "patternLength")]
    pub pattern_length: f64,
}

// ── Layer table ──

#[derive(Debug, Serialize)]
pub struct LayerTableOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    pub layers: BTreeMap<String, LayerEntryOutput>,
}

#[derive(Debug, Serialize)]
pub struct LayerEntryOutput {
    pub name: String,
    pub frozen: bool,
    pub visible: bool,
    #[serde(rename = "colorIndex")]
    pub color_index: i16,
    pub color: u32,
    #[serde(rename = "lineType")]
    pub line_type: String,
    #[serde(rename = "lineWeight")]
    pub line_weight: i16,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

// ── Text style table ──

#[derive(Debug, Serialize)]
pub struct StyleTableOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    pub styles: BTreeMap<String, StyleEntryOutput>,
}

#[derive(Debug, Serialize)]
pub struct StyleEntryOutput {
    #[serde(rename = "styleName")]
    pub style_name: String,
    #[serde(rename = "fixedTextHeight")]
    pub fixed_text_height: f64,
    #[serde(rename = "widthFactor")]
    pub width_factor: f64,
    #[serde(rename = "obliqueAngle")]
    pub oblique_angle: f64,
    #[serde(rename = "lastHeight")]
    pub last_height: f64,
    pub font: String,
    #[serde(rename = "bigFont")]
    pub big_font: String,
}

// ── Dimension style table ──

#[derive(Debug, Serialize)]
pub struct DimStyleTableOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    #[serde(rename = "dimStyles")]
    pub dim_styles: BTreeMap<String, DimStyleEntryOutput>,
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct DimStyleEntryOutput {
    pub name: String,
    pub DIMSCALE: f64,
    pub DIMASZ: f64,
    pub DIMEXO: f64,
    pub DIMEXE: f64,
    pub DIMTXT: f64,
    pub DIMGAP: f64,
    pub DIMCLRT: i16,
    pub DIMDEC: i16,
    pub DIMDLE: f64,
    pub DIMDLI: f64,
    pub DIMLFAC: f64,
    pub DIMRND: f64,
    pub DIMTAD: i16,
    pub DIMTIH: bool,
    pub DIMTOH: bool,
    pub DIMTIX: bool,
    pub DIMTOFL: bool,
}

// ────────────────────────── Blocks ──────────────────────────

#[derive(Debug, Serialize)]
pub struct BlockOutput {
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    pub layer: String,
    pub name: String,
    pub position: Point3DOutput,
    pub name2: String,
    #[serde(rename = "xrefPath")]
    pub xref_path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityOutput>,
}

// ────────────────────────── Entities ──────────────────────────

/// A single entity output – uses `#[serde(flatten)]` to inline geometry fields.
#[derive(Debug, Serialize)]
pub struct EntityOutput {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub handle: String,
    #[serde(rename = "ownerHandle")]
    pub owner_handle: String,
    pub layer: String,
    #[serde(rename = "lineType", skip_serializing_if = "Option::is_none")]
    pub line_type: Option<String>,
    #[serde(rename = "colorIndex", skip_serializing_if = "Option::is_none")]
    pub color_index: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(flatten)]
    pub geometry: EntityGeometryOutput,
}

/// Entity geometry – flat-serialized per entity type.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EntityGeometryOutput {
    Line {
        vertices: Vec<Point3DOutput>,
    },
    Circle {
        center: Point3DOutput,
        radius: f64,
    },
    Arc {
        center: Point3DOutput,
        radius: f64,
        #[serde(rename = "startAngle")]
        start_angle: f64,
        #[serde(rename = "endAngle")]
        end_angle: f64,
    },
    Ellipse {
        center: Point3DOutput,
        #[serde(rename = "majorAxisEndPoint")]
        major_axis_end_point: Point3DOutput,
        #[serde(rename = "axisRatio")]
        axis_ratio: f64,
        #[serde(rename = "startAngle")]
        start_angle: f64,
        #[serde(rename = "endAngle")]
        end_angle: f64,
    },
    LwPolyline {
        vertices: Vec<Point2DOutput>,
        shape: bool,
        #[serde(rename = "hasContinuousLinetypePattern")]
        has_continuous_linetype_pattern: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
    },
    Polyline {
        vertices: Vec<Point3DOutput>,
        shape: bool,
    },
    Spline {
        degree: i32,
        knots: Vec<f64>,
        #[serde(rename = "controlPoints")]
        control_points: Vec<Point3DOutput>,
        #[serde(rename = "fitPoints")]
        fit_points: Vec<Point3DOutput>,
    },
    Text {
        #[serde(rename = "startPoint")]
        start_point: Point3DOutput,
        #[serde(rename = "textHeight")]
        text_height: f64,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
        #[serde(rename = "xScale", skip_serializing_if = "Option::is_none")]
        x_scale: Option<f64>,
        #[serde(rename = "styleName", skip_serializing_if = "Option::is_none")]
        style_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        halign: Option<i16>,
        #[serde(rename = "endPoint", skip_serializing_if = "Option::is_none")]
        end_point: Option<Point3DOutput>,
    },
    MText {
        #[serde(rename = "insertionPoint")]
        insertion_point: Point3DOutput,
        #[serde(rename = "textHeight")]
        text_height: f64,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(rename = "attachmentPoint", skip_serializing_if = "Option::is_none")]
        attachment_point: Option<i16>,
    },
    Insert {
        name: String,
        position: Point3DOutput,
        #[serde(rename = "xScale", skip_serializing_if = "Option::is_none")]
        x_scale: Option<f64>,
        #[serde(rename = "yScale", skip_serializing_if = "Option::is_none")]
        y_scale: Option<f64>,
        #[serde(rename = "zScale", skip_serializing_if = "Option::is_none")]
        z_scale: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
    },
    Dimension {
        #[serde(rename = "blockName")]
        block_name: String,
        #[serde(rename = "insertionPoint")]
        insertion_point: Point3DOutput,
    },
    Solid {
        points: Vec<Point3DOutput>,
    },
    Hatch {
        #[serde(rename = "patternName")]
        pattern_name: String,
        #[serde(rename = "isSolid")]
        is_solid: bool,
        #[serde(rename = "boundaryLoops")]
        boundary_loops: Vec<HatchBoundaryLoopOutput>,
    },
    Unsupported {
        #[serde(rename = "entityType")]
        original_type: String,
    },
}

// ── Hatch boundary ──

#[derive(Debug, Serialize)]
pub struct HatchBoundaryLoopOutput {
    #[serde(rename = "type")]
    pub loop_type: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polyline: Option<HatchPolylineOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<HatchEdgeOutput>>,
}

#[derive(Debug, Serialize)]
pub struct HatchPolylineOutput {
    pub vertices: Vec<Point3DOutput>,
    #[serde(rename = "isClosed")]
    pub is_closed: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum HatchEdgeOutput {
    Line {
        start: Point2DOutput,
        end: Point2DOutput,
    },
    CircularArc {
        center: Point2DOutput,
        radius: f64,
        #[serde(rename = "startAngle")]
        start_angle: f64,
        #[serde(rename = "endAngle")]
        end_angle: f64,
        #[serde(rename = "counterClockwise")]
        counter_clockwise: bool,
    },
    EllipticArc {
        center: Point2DOutput,
        #[serde(rename = "majorAxisEndPoint")]
        major_axis_end_point: Point2DOutput,
        #[serde(rename = "minorAxisRatio")]
        minor_axis_ratio: f64,
        #[serde(rename = "startAngle")]
        start_angle: f64,
        #[serde(rename = "endAngle")]
        end_angle: f64,
        #[serde(rename = "counterClockwise")]
        counter_clockwise: bool,
    },
    Spline {
        degree: i32,
        knots: Vec<f64>,
        #[serde(rename = "controlPoints")]
        control_points: Vec<Point2DOutput>,
    },
}

// ────────────────────────── ACI → RGB conversion ──────────────────────────

/// AutoCAD Color Index to 24-bit decimal RGB.
pub fn aci_to_rgb_decimal(index: u8) -> u32 {
    // Standard ACI colors (indices 1-9)
    match index {
        0 => 0,              // ByBlock (black)
        1 => 16711680,       // Red     (255,0,0)
        2 => 16776960,       // Yellow  (255,255,0)
        3 => 65280,          // Green   (0,255,0)
        4 => 65535,          // Cyan    (0,255,255)
        5 => 255,            // Blue    (0,0,255)
        6 => 16711935,       // Magenta (255,0,255)
        7 => 16777215,       // White   (255,255,255)
        8 => 8421504,        // Dark grey (128,128,128)
        9 => 12632256,       // Light grey (192,192,192)
        250 => 3355443,      // (51,51,51)
        251 => 5000268,      // (76,76,76)
        252 => 6710886,      // (102,102,102)
        253 => 8421504,      // (128,128,128)
        254 => 14079702,     // (214,214,214) - often used
        255 => 16777215,     // White
        _ => 16777215,       // Fallback to white for unmapped indices
    }
}

/// Convert Color enum to (colorIndex, color_decimal) for output.
pub fn color_to_output(color: &acadrust::Color) -> (Option<i16>, Option<u32>) {
    match color {
        acadrust::Color::ByLayer => (None, None),
        acadrust::Color::ByBlock => (Some(0), Some(0)),
        acadrust::Color::Index(i) => (Some(*i as i16), Some(aci_to_rgb_decimal(*i))),
        acadrust::Color::Rgb { r, g, b } => {
            let decimal = (*r as u32) << 16 | (*g as u32) << 8 | (*b as u32);
            (None, Some(decimal))
        }
    }
}

/// Convert Color enum to colorIndex for layer output (always produces an index).
pub fn color_to_layer_index(color: &acadrust::Color) -> i16 {
    match color {
        acadrust::Color::ByLayer => 7,
        acadrust::Color::ByBlock => 0,
        acadrust::Color::Index(i) => *i as i16,
        acadrust::Color::Rgb { .. } => 7,
    }
}

/// Convert Color to decimal RGB for layer output.
pub fn color_to_layer_rgb(color: &acadrust::Color) -> u32 {
    match color {
        acadrust::Color::Index(i) => aci_to_rgb_decimal(*i),
        acadrust::Color::Rgb { r, g, b } => (*r as u32) << 16 | (*g as u32) << 8 | (*b as u32),
        _ => 16777215, // white
    }
}

/// Convert LineWeight to numeric i16 value for output.
pub fn line_weight_to_output(lw: &acadrust::LineWeight) -> i16 {
    match lw {
        acadrust::LineWeight::ByLayer => -1,
        acadrust::LineWeight::ByBlock => -2,
        acadrust::LineWeight::Default => -3,
        acadrust::LineWeight::Value(v) => *v,
    }
}
