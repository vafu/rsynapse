#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) enum RegionShape {
    Rectangle,
    Rounded {
        radius: i32,
        inset: i32,
        corner_guard: i32,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegionSize {
    pub(crate) width: i32,
    pub(crate) height: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct RegionRectangle {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl RegionRectangle {
    #[cfg(test)]
    pub(crate) const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn rounded_rectangles_with_corner_guard(
        self,
        radius: i32,
        corner_guard: i32,
    ) -> Vec<Self> {
        let radius = radius.max(0).min(self.width / 2).min(self.height / 2);
        if radius == 0 {
            return vec![self];
        }
        let corner_guard = corner_guard.max(0);

        let mut top_bands = Vec::with_capacity(radius as usize);
        for row in 0..radius {
            let offset = rounded_corner_offset(radius, row)
                + rounded_corner_guard(radius, row, corner_guard);
            push_rounded_band(self, row, row + 1, offset, &mut top_bands);
        }

        let mut rectangles = Vec::with_capacity((top_bands.len() * 2) + 1);
        rectangles.extend(top_bands.iter().copied());

        let center_height = self.height - radius * 2;
        if center_height > 0 {
            rectangles.push(Self {
                x: self.x,
                y: self.y + radius,
                width: self.width,
                height: center_height,
            });
        }

        for band in top_bands.iter().rev() {
            rectangles.push(Self {
                x: band.x,
                y: self.y + self.height - (band.y - self.y) - band.height,
                width: band.width,
                height: band.height,
            });
        }

        rectangles
    }

    pub(crate) fn inset(self, inset: i32) -> Option<Self> {
        let inset = inset.max(0);
        if inset == 0 {
            return Some(self);
        }

        let removed = inset.saturating_mul(2);
        if self.width <= removed || self.height <= removed {
            return None;
        }

        Some(Self {
            x: self.x + inset,
            y: self.y + inset,
            width: self.width - removed,
            height: self.height - removed,
        })
    }

    pub(crate) fn translated_and_clipped(
        self,
        x: i32,
        y: i32,
        surface_size: RegionSize,
    ) -> Option<Self> {
        if self.width <= 0 || self.height <= 0 {
            return None;
        }

        let surface_width = i64::from(surface_size.width.max(0));
        let surface_height = i64::from(surface_size.height.max(0));
        let left = i64::from(self.x) + i64::from(x);
        let top = i64::from(self.y) + i64::from(y);
        let right = left + i64::from(self.width);
        let bottom = top + i64::from(self.height);

        let left = left.clamp(0, surface_width);
        let top = top.clamp(0, surface_height);
        let right = right.clamp(0, surface_width);
        let bottom = bottom.clamp(0, surface_height);
        if right <= left || bottom <= top {
            return None;
        }

        Some(Self {
            x: left as i32,
            y: top as i32,
            width: (right - left) as i32,
            height: (bottom - top) as i32,
        })
    }
}

pub(crate) fn append_region_rectangles(
    rectangle: RegionRectangle,
    shape: RegionShape,
    rectangles: &mut Vec<RegionRectangle>,
) {
    match shape {
        RegionShape::Rectangle => rectangles.push(rectangle),
        RegionShape::Rounded {
            radius,
            inset,
            corner_guard,
        } => {
            let inset = inset.max(0);
            let radius = radius.saturating_sub(inset);
            if let Some(rectangle) = rectangle.inset(inset) {
                rectangles
                    .extend(rectangle.rounded_rectangles_with_corner_guard(radius, corner_guard));
            }
        }
    }
}

fn push_rounded_band(
    rectangle: RegionRectangle,
    start_row: i32,
    end_row: i32,
    offset: i32,
    rectangles: &mut Vec<RegionRectangle>,
) {
    let height = end_row - start_row;
    let width = rectangle.width - offset * 2;
    if height <= 0 || width <= 0 {
        return;
    }

    rectangles.push(RegionRectangle {
        x: rectangle.x + offset,
        y: rectangle.y + start_row,
        width,
        height,
    });
}

fn rounded_corner_offset(radius: i32, row: i32) -> i32 {
    let radius = radius as f64;
    let y = row as f64 + 0.5;
    let dy = radius - y;
    (radius - (radius * radius - dy * dy).sqrt()).ceil() as i32
}

fn rounded_corner_guard(radius: i32, row: i32, corner_guard: i32) -> i32 {
    let corner_guard = corner_guard.max(0);
    if radius <= 1 || corner_guard == 0 {
        return 0;
    }

    let denominator = radius - 1;
    let remaining_corner_rows = radius - row - 1;
    if remaining_corner_rows <= 0 {
        return 0;
    }

    (corner_guard * remaining_corner_rows + denominator / 2) / denominator
}
