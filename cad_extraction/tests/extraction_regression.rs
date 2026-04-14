use std::{path::PathBuf, time::Instant};

use acadrust::{Arc, CadDocument, Circle, EntityType, Line, Point, Vector3};
use cad_extraction::{
    extraction::extractor::extract_document,
    ui::panels::WGPU_RECOMMENDATION_THRESHOLD,
};

#[test]
fn extract_document_creates_layer_index_and_bounds() {
    let mut document = CadDocument::new();

    let mut first = Line::from_points(Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0));
    first.common.layer = "WALLS".to_owned();
    document.add_entity(EntityType::Line(first)).unwrap();

    let mut second = Circle::from_center_radius(Vector3::new(5.0, 5.0, 0.0), 3.0);
    second.common.layer = "DOORS".to_owned();
    document.add_entity(EntityType::Circle(second)).unwrap();

    let output = extract_document(document, PathBuf::from("synthetic.dxf"), 1);

    assert_eq!(output.stats.total_entities, 2);
    assert!(output.layer_index.contains_key("WALLS"));
    assert!(output.layer_index.contains_key("DOORS"));
    assert!(output.bounds.is_some());
}

#[test]
fn extract_document_marks_unsupported_entities() {
    let mut document = CadDocument::new();
    let mut arc = Arc::from_center_radius_angles(Vector3::new(0.0, 0.0, 0.0), 4.0, 0.0, 1.57);
    arc.common.layer = "ANNO".to_owned();
    document.add_entity(EntityType::Arc(arc)).unwrap();

    // Point remains unsupported in scene extraction and should be tracked.
    let mut point = Point::from_coords(1.0, 1.0, 0.0);
    point.common.layer = "ANNO".to_owned();
    document.add_entity(EntityType::Point(point)).unwrap();

    let output = extract_document(document, PathBuf::from("unsupported.dxf"), 1);
    assert_eq!(output.stats.total_entities, 2);
    assert_eq!(output.stats.ignored_entities, 1);
}

#[test]
fn extraction_perf_baseline_generated_geometry() {
    let mut document = CadDocument::new();
    for index in 0..20_000usize {
        let x = index as f64;
        let mut line = Line::from_points(Vector3::new(x, 0.0, 0.0), Vector3::new(x, 10.0, 0.0));
        line.common.layer = "PERF".to_owned();
        document.add_entity(EntityType::Line(line)).unwrap();
    }

    let start = Instant::now();
    let output = extract_document(document, PathBuf::from("perf.dxf"), 0);
    let elapsed = start.elapsed().as_millis();

    assert_eq!(output.stats.total_entities, 20_000);
    assert!(elapsed < 10_000, "extraction took too long: {elapsed}ms");
}

#[test]
fn wgpu_gate_threshold_is_defined() {
    assert!(WGPU_RECOMMENDATION_THRESHOLD >= 100_000);
}
