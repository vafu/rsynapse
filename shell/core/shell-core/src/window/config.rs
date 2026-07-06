use super::{
    Anchors, BackgroundEffect, BackgroundEffectRegion, Edge, ExclusiveZone, Layer, SurfaceMargins,
    WindowConfig,
};

impl SurfaceMargins {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn uniform(value: i32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

impl Anchors {
    pub const NONE: Self = Self {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };

    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };

    pub const fn new(top: bool, right: bool, bottom: bool, left: bool) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn with_edge(mut self, edge: Edge) -> Self {
        match edge {
            Edge::Top => self.top = true,
            Edge::Right => self.right = true,
            Edge::Bottom => self.bottom = true,
            Edge::Left => self.left = true,
        }
        self
    }
}

impl WindowConfig {
    pub const fn new(layer: Layer) -> Self {
        Self {
            layer,
            anchors: Anchors::NONE,
            surface_margins: SurfaceMargins::ZERO,
            exclusive_zone: ExclusiveZone::None,
            background_effect: BackgroundEffect::None,
            namespace: None,
            keyboard_interactive: false,
        }
    }

    pub const fn with_anchors(mut self, anchors: Anchors) -> Self {
        self.anchors = anchors;
        self
    }

    pub const fn with_surface_margins(mut self, margins: SurfaceMargins) -> Self {
        self.surface_margins = margins;
        self
    }

    pub const fn with_exclusive_zone(mut self, exclusive_zone: ExclusiveZone) -> Self {
        self.exclusive_zone = exclusive_zone;
        self
    }

    pub const fn with_fixed_exclusive_zone(mut self, exclusive_zone: i32) -> Self {
        self.exclusive_zone = ExclusiveZone::Fixed(exclusive_zone);
        self
    }

    pub const fn with_auto_exclusive_zone(mut self) -> Self {
        self.exclusive_zone = ExclusiveZone::Auto;
        self
    }

    pub const fn with_background_effect(mut self, background_effect: BackgroundEffect) -> Self {
        self.background_effect = background_effect;
        self
    }

    pub const fn with_background_blur_region(mut self, region: BackgroundEffectRegion) -> Self {
        self.background_effect = BackgroundEffect::Blur(region);
        self
    }

    pub const fn with_background_blur(self) -> Self {
        self.with_background_blur_region(BackgroundEffectRegion::Surface)
    }

    pub const fn with_background_blur_for_css_classes(
        mut self,
        classes: &'static [&'static str],
    ) -> Self {
        self.background_effect =
            BackgroundEffect::Blur(BackgroundEffectRegion::CssClasses(classes));
        self
    }

    pub const fn with_rounded_background_blur_for_css_classes(
        mut self,
        classes: &'static [&'static str],
        radius: i32,
    ) -> Self {
        self.background_effect =
            BackgroundEffect::Blur(BackgroundEffectRegion::RoundedCssClasses { classes, radius });
        self
    }

    pub const fn with_namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    pub const fn with_keyboard_interactivity(mut self, interactive: bool) -> Self {
        self.keyboard_interactive = interactive;
        self
    }
}
