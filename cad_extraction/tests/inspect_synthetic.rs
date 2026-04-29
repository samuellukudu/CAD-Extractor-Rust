use std::path::PathBuf;
use cad_extraction::extraction::extractor::extract_document;
use acadrust::{CadDocument, EntityType, Line, Vector3};

#[test]
fn inspect() {
    let mut document = CadDocument::new();
    let mut first = Line::from_points(Vector3::new(0.0, 0.0, 0.0), Vector3::new(10.0, 0.0, 0.0));
    first.common.layer = "WALLS".to_owned();
    document.add_entity(EntityType::Line(first)).unwrap();

    let output = extract_document(document, PathBuf::from("synthetic.dxf"), 1);
    for e in &output.entities {
        println!("entity block: {:?}", e.block_name);
    }
}
