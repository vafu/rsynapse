use crate::{
    BackgroundEffect, BackgroundEffectRegion,
    region::{RegionRectangle, RegionShape, RegionSize, append_region_rectangles},
};

#[test]
fn rounded_region_uses_one_pixel_corner_bands() {
    let rectangles = RegionRectangle::new(0, 0, 20, 20).rounded_rectangles_with_corner_guard(4, 0);

    assert_eq!(
        rectangles,
        vec![
            RegionRectangle::new(3, 0, 14, 1),
            RegionRectangle::new(1, 1, 18, 1),
            RegionRectangle::new(1, 2, 18, 1),
            RegionRectangle::new(1, 3, 18, 1),
            RegionRectangle::new(0, 4, 20, 12),
            RegionRectangle::new(1, 16, 18, 1),
            RegionRectangle::new(1, 17, 18, 1),
            RegionRectangle::new(1, 18, 18, 1),
            RegionRectangle::new(3, 19, 14, 1),
        ]
    );
}

#[test]
fn inset_rounded_region_shrinks_before_rounding() {
    let mut rectangles = Vec::new();
    append_region_rectangles(
        RegionRectangle::new(0, 0, 20, 20),
        RegionShape::Rounded {
            radius: 4,
            inset: 2,
            corner_guard: 0,
        },
        &mut rectangles,
    );

    assert_eq!(
        rectangles,
        vec![
            RegionRectangle::new(3, 2, 14, 1),
            RegionRectangle::new(3, 3, 14, 1),
            RegionRectangle::new(2, 4, 16, 12),
            RegionRectangle::new(3, 16, 14, 1),
            RegionRectangle::new(3, 17, 14, 1),
        ]
    );
}

#[test]
fn corner_guard_shrinks_only_corner_bands() {
    let rectangles = RegionRectangle::new(0, 0, 20, 20).rounded_rectangles_with_corner_guard(4, 1);

    assert_eq!(
        rectangles,
        vec![
            RegionRectangle::new(4, 0, 12, 1),
            RegionRectangle::new(2, 1, 16, 1),
            RegionRectangle::new(1, 2, 18, 1),
            RegionRectangle::new(1, 3, 18, 1),
            RegionRectangle::new(0, 4, 20, 12),
            RegionRectangle::new(1, 16, 18, 1),
            RegionRectangle::new(1, 17, 18, 1),
            RegionRectangle::new(2, 18, 16, 1),
            RegionRectangle::new(4, 19, 12, 1),
        ]
    );
}

#[test]
fn translated_region_clips_to_surface() {
    let surface_size = RegionSize {
        width: 20,
        height: 12,
    };

    assert_eq!(
        RegionRectangle::new(2, 3, 10, 4).translated_and_clipped(5, 6, surface_size),
        Some(RegionRectangle::new(7, 9, 10, 3))
    );
    assert_eq!(
        RegionRectangle::new(0, 0, 10, 10).translated_and_clipped(-3, 8, surface_size),
        Some(RegionRectangle::new(0, 8, 7, 4))
    );
    assert_eq!(
        RegionRectangle::new(0, 0, 10, 10).translated_and_clipped(30, 0, surface_size),
        None
    );
}

#[test]
fn background_effect_regions_are_copyable_config_values() {
    const CLASSES: &[&str] = &["blur"];
    const REGIONS: &[BackgroundEffectRegion] = &[
        BackgroundEffectRegion::RoundedCssClasses {
            classes: CLASSES,
            radius: 12,
        },
        BackgroundEffectRegion::CornerGuardRoundedCssClasses {
            classes: CLASSES,
            radius: 12,
            corner_guard: 1,
        },
    ];

    assert_eq!(
        BackgroundEffect::Blur(BackgroundEffectRegion::Regions(REGIONS)),
        BackgroundEffect::Blur(BackgroundEffectRegion::Regions(REGIONS))
    );
}
