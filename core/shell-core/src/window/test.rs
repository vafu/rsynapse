use super::{Anchors, Edge, ExclusiveZone, Layer, SurfaceMargins, WindowConfig};

#[test]
fn anchors_can_be_built_from_generic_edges() {
    let anchors = Anchors::NONE
        .with_edge(Edge::Top)
        .with_edge(Edge::Right)
        .with_edge(Edge::Left);

    assert_eq!(anchors, Anchors::new(true, true, false, true));
}

#[test]
fn config_builders_preserve_explicit_values() {
    let anchors = Anchors::new(false, true, true, true);
    let config = WindowConfig::new(Layer::Bottom)
        .with_anchors(anchors)
        .with_surface_margins(SurfaceMargins::uniform(8))
        .with_fixed_exclusive_zone(12)
        .with_namespace("custom")
        .with_keyboard_interactivity(true);

    assert_eq!(config.anchors, anchors);
    assert_eq!(config.surface_margins, SurfaceMargins::uniform(8));
    assert_eq!(config.exclusive_zone, ExclusiveZone::Fixed(12));
    assert_eq!(config.namespace, Some("custom"));
    assert!(config.keyboard_interactive);
}

#[test]
fn config_supports_auto_exclusive_zone() {
    let config = WindowConfig::new(Layer::Top).with_auto_exclusive_zone();

    assert_eq!(config.exclusive_zone, ExclusiveZone::Auto);
}
