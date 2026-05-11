use acadrust::{CadDocument, DwgReader, DxfReader};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: inspect_header <file>");
        return;
    }
    let path = &args[1];
    
    let doc = if path.to_lowercase().ends_with(".dwg") {
        DwgReader::from_file(path).unwrap().read().unwrap()
    } else {
        DxfReader::from_file(path).unwrap().read().unwrap()
    };
    
    println!("Header: {:#?}", doc.header);
}
