//! GTK4 helper for Wayland `ext-background-effect-v1`.
//!
//! The crate is intentionally small: it installs compositor-owned background
//! effects on an existing GTK window and keeps the Wayland protocol handles
//! alive for that window. Unsupported backends or compositors are no-ops.

mod effect;
mod region;

pub use effect::apply_background_effect;

/// Area of a GTK window that should receive a compositor background effect.
///
/// Wayland regions are rectilinear, so rounded regions are approximated with
/// one-pixel horizontal bands through the curved corners. Inset rounded regions
/// are useful when a GTK widget paints translucent antialiasing or shadows
/// around its CSS box and the compositor blur should stay inside that painted
/// edge.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BackgroundEffectRegion {
    /// Apply the effect to the whole GTK surface.
    Surface,
    /// Apply the effect to all drawable descendants with any matching CSS class.
    CssClasses(&'static [&'static str]),
    /// Apply the effect to matching CSS-class widgets with rounded-rectangle
    /// region approximation.
    RoundedCssClasses {
        classes: &'static [&'static str],
        radius: i32,
    },
    /// Apply the effect to matching CSS-class widgets with rounded-rectangle
    /// approximation, adding a tapered `corner_guard` only to the rounded
    /// corner cutouts. Straight edges still extend to the widget bounds.
    CornerGuardRoundedCssClasses {
        classes: &'static [&'static str],
        radius: i32,
        corner_guard: i32,
    },
    /// Apply the effect to matching CSS-class widgets after shrinking their
    /// bounds by `inset` pixels on each edge, then approximating the remaining
    /// rounded rectangle.
    InsetRoundedCssClasses {
        classes: &'static [&'static str],
        radius: i32,
        inset: i32,
    },
    /// Apply the effect to several region descriptors on the same GTK surface.
    ///
    /// This is useful when a window contains several independently rounded
    /// visual elements with different radii.
    Regions(&'static [BackgroundEffectRegion]),
}

/// Background effect requested for a GTK window.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BackgroundEffect {
    None,
    Blur(BackgroundEffectRegion),
}

impl BackgroundEffectRegion {
    pub(crate) fn needs_layout_refresh(self) -> bool {
        // GTK/layer-shell surfaces can briefly report their initial 1x1 size
        // before the compositor configures the final surface. CSS-class
        // regions also depend on GTK allocation changes.
        match self {
            Self::Surface
            | Self::CssClasses(_)
            | Self::RoundedCssClasses { .. }
            | Self::CornerGuardRoundedCssClasses { .. }
            | Self::InsetRoundedCssClasses { .. } => true,
            Self::Regions(regions) => regions.iter().any(|region| region.needs_layout_refresh()),
        }
    }
}

#[cfg(test)]
mod test;
