use acadrust::{Arc, Vector3};
#[test]
fn test_arc() {
    let mut arc = Arc::default();
    arc.start_angle = 90.0;
    println!("angle: {}", arc.start_angle);
}
