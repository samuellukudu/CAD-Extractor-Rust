use eframe::egui;
fn main() {
    let t = egui::TextShape::new(egui::Pos2::ZERO, Default::default(), egui::Color32::WHITE);
    let _a = t.angle;
}
