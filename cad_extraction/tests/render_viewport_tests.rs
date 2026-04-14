use cad_extraction::{
    extraction::models::Point2,
    render::painter::ViewportState,
};
use egui::{Pos2, Rect};

#[test]
fn viewport_uses_cad_y_up_mapping() {
    let viewport = ViewportState {
        zoom: 2.0,
        pan: egui::Vec2::new(100.0, 100.0),
    };

    let screen = viewport.world_to_screen(Point2::new(10.0, 5.0));
    assert_eq!(screen, Pos2::new(120.0, 90.0));
}

#[test]
fn viewport_world_screen_roundtrip_is_stable() {
    let mut viewport = ViewportState::default();
    viewport.zoom = 3.5;
    viewport.pan = egui::Vec2::new(240.0, 260.0);

    let world = Point2::new(12.5, -8.75);
    let screen = viewport.world_to_screen(world);
    let reconstructed = viewport.screen_to_world(screen);
    assert!((reconstructed.x - world.x).abs() < 1e-8);
    assert!((reconstructed.y - world.y).abs() < 1e-8);
}

#[test]
fn fit_to_bounds_places_drawing_with_y_up() {
    let mut viewport = ViewportState::default();
    let bounds = cad_extraction::extraction::models::Bounds2D {
        min: Point2::new(0.0, 0.0),
        max: Point2::new(100.0, 100.0),
    };
    let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(500.0, 500.0));
    viewport.fit_to_bounds(Some(bounds), rect);

    let top_right = viewport.world_to_screen(Point2::new(100.0, 100.0));
    let bottom_left = viewport.world_to_screen(Point2::new(0.0, 0.0));
    assert!(top_right.y < bottom_left.y, "Y axis should be upward in world");
}
