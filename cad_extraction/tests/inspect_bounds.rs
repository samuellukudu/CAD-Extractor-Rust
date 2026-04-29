use std::path::PathBuf;
use cad_extraction::extraction::extractor::extract_file;
use cad_extraction::render::painter::compute_drawing_bounds;

#[test]
fn inspect() {
    let path = PathBuf::from("/Users/samuellukudu/QilaiCo/End2End/Graphs/data/AutoCAD_DWG/四清单合一/20250702-景观说明+景观全图_t3.dwg");
    if !path.exists() { return; }
    let drawing = extract_file(&path).unwrap();
    let bounds = compute_drawing_bounds(&drawing);
    println!("Computed bounds: {:?}", bounds);
}
