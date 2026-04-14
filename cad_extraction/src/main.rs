use cad_extraction::ui::app::CadViewerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "CAD Extraction Viewer",
        options,
        Box::new(|cc| Ok(Box::new(CadViewerApp::new(cc)))),
    )
}
