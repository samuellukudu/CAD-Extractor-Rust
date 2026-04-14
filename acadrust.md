Crate acadrust Copy item path
Source
Search
Settings
Help

Summary
acadrust
A pure Rust library for reading and writing CAD files in DXF and DWG formats.

acadrust provides comprehensive support for both file formats with a focus on correctness, type safety, and completeness. Inspired by ACadSharp, it brings full-featured CAD file manipulation to the Rust ecosystem.

Highlights
DXF — Read and write ASCII and Binary DXF (R12 through R2018+)
DWG — Read and write native DWG binary files (R13 through R2018+)
41 entity types, 9 table types, 20+ non-graphical objects
ACIS/SAT/SAB — Parse and write ACIS solid-model data (SAT text and SAB binary); parametric primitive builders for box, wedge, pyramid, cylinder, cone, sphere, and torus
Type safe — strongly-typed entities, tables, and enums
Failsafe mode — error-tolerant parsing that collects diagnostics
Encoding support — automatic code page detection for pre-2007 files
Serde support — optional Serialize/Deserialize for all types (enable the serde feature)
Feature Flags
Feature	Description
serde	Enables serde::Serialize and serde::Deserialize on all document types
[dependencies]
acadrust = { version = "0.3.3", features = ["serde"] }
Serialize an entity to JSON
ⓘ
use acadrust::entities::Line;

let line = Line::from_coords(0.0, 0.0, 0.0, 100.0, 50.0, 0.0);
let json = serde_json::to_string_pretty(&line).unwrap();
println!("{json}");
Round-trip a full document
ⓘ
use acadrust::{CadDocument, DxfReader};

let doc = DxfReader::from_file("input.dxf")?.read()?;

// Serialize
let json = serde_json::to_string(&doc).unwrap();

// Deserialize
let doc2: CadDocument = serde_json::from_str(&json).unwrap();
assert_eq!(doc2.entities().count(), doc.entities().count());
See the examples/serde_json.rs example for more patterns including web-API-style entity lists.

Quick Start — DXF
ⓘ
use acadrust::{CadDocument, DxfReader, DxfWriter};

// Read
let doc = DxfReader::from_file("input.dxf")?.read()?;
println!("Entities: {}", doc.entities().count());

// Write
DxfWriter::new(&doc).write_to_file("output.dxf")?;
Quick Start — DWG
ⓘ
use acadrust::{CadDocument, DwgWriter};
use acadrust::io::dwg::DwgReader;
use acadrust::entities::*;
use acadrust::types::{Color, Vector3};

// Read
let mut reader = DwgReader::from_file("input.dwg")?;
let doc = reader.read()?;

// Create and write
let mut doc = CadDocument::new();
let mut line = Line::from_coords(0.0, 0.0, 0.0, 100.0, 50.0, 0.0);
line.common.color = Color::RED;
doc.add_entity(EntityType::Line(line))?;
DwgWriter::write_to_file("output.dwg", &doc)?;
Module Overview
Module	Contents
document	CadDocument — the central drawing container
entities	41 graphical entity types (Line, Circle, Spline, …)
tables	Table entries (Layer, LineType, TextStyle, DimStyle, …)
objects	Non-graphical objects (dictionaries, layouts, styles)
types	Primitives (Vector3, Color, Handle, DxfVersion, …)
io	Readers and writers for DXF and DWG
entities::acis	ACIS/SAT/SAB solid-model parser, writer, and primitive builders
classes	DXF class definitions (CLASSES section)
xdata	Extended data (XData) attached to entities
error	Error types (DxfError) and Result alias
notification	Structured parse diagnostics
File Version Support
Code	AutoCAD	DXF	DWG
AC1009	R12	R/W	—
AC1012	R13	R/W	R/W
AC1014	R14	R/W	R/W
AC1015	2000	R/W	R/W
AC1018	2004	R/W	R/W
AC1021	2007	R/W	R/W
AC1024	2010	R/W	R/W
AC1027	2013	R/W	R/W
AC1032	2018+	R/W	R/W
Re-exports
pub use error::DxfError;
pub use error::Result;
pub use types::DxfVersion;
pub use types::BoundingBox2D;
pub use types::BoundingBox3D;
pub use types::Color;
pub use types::Handle;
pub use types::LineWeight;
pub use types::Transparency;
pub use types::Vector2;
pub use types::Vector3;
pub use entities::Arc;
pub use entities::Circle;
pub use entities::Ellipse;
pub use entities::Entity;
pub use entities::EntityType;
pub use entities::Line;
pub use entities::LwPolyline;
pub use entities::MText;
pub use entities::Point;
pub use entities::Polyline;
pub use entities::Spline;
pub use entities::Text;
pub use tables::AppId;
pub use tables::BlockRecord;
pub use tables::DimStyle;
pub use tables::Layer;
pub use tables::LineType;
pub use tables::Table;
pub use tables::TableEntry;
pub use tables::TextStyle;
pub use tables::Ucs;
pub use tables::VPort;
pub use tables::View;
pub use document::CadDocument;
pub use io::dxf::DxfReader;
pub use io::dxf::DxfWriter;
pub use io::dwg::DwgReader;
pub use io::dwg::DwgReadOptions;
pub use io::dwg::DwgWriter;
pub use entities::acis::SatDocument;
pub use entities::acis::SatHeader;
pub use entities::acis::SatVersion;
pub use entities::acis::SatRecord;
pub use entities::acis::SatPointer;
pub use entities::acis::SatToken;
pub use entities::acis::SatParser;
pub use entities::acis::SatWriter;
pub use entities::acis::SabWriter;
pub use entities::acis::SabReader;
pub use entities::acis::primitives;
Modules
classes
DXF class definitions (CLASSES section)
document
Central CAD document structure.
entities
Graphical entity types.
error
Error types for acadrust.
io
File I/O for DXF and DWG formats.
notification
Parse notification / diagnostic system.
objects
Non-graphical objects (OBJECTS section)
tables
Table types and the generic Table container.
types
Core types used throughout acadrust.
xdata
Extended Data (XDATA) support
Constants
VERSION
Library version