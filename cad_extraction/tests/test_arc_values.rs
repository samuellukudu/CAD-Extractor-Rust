use std::path::PathBuf;
use cad_extraction::extraction::extractor::extract_file;
use cad_extraction::extraction::models::SceneGeometry;

#[test]
fn inspect() {
    let path = PathBuf::from("/Users/samuellukudu/QilaiCo/End2End/Graphs/data/AutoCAD_DWG/四清单合一/20250702-景观说明+景观全图_t3.dwg");
    if !path.exists() { return; }
    let drawing = extract_file(&path).unwrap();
    let mut count = 0;
    for e in &drawing.entities {
        if let SceneGeometry::Arc { start_angle, end_angle, .. } = e.geometry {
            println!("Arc start: {}, end: {}", start_angle, end_angle);
            count += 1;
            if count > 5 { break; }
        }
    }
}
