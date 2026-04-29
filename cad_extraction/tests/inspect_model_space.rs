use std::path::PathBuf;
use cad_extraction::extraction::extractor::extract_file;

#[test]
fn inspect() {
    let path = PathBuf::from("/Users/samuellukudu/QilaiCo/End2End/Graphs/data/AutoCAD_DWG/四清单合一/20250702-景观说明+景观全图_t3.dwg");
    if !path.exists() {
        return;
    }
    let drawing = extract_file(&path).unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for entity in &drawing.entities {
        let name = entity.block_name.clone().unwrap_or_else(|| "<None>".to_string());
        *counts.entry(name).or_insert(0) += 1;
    }
    for (name, count) in counts {
        println!("{}: {}", name, count);
    }
}
