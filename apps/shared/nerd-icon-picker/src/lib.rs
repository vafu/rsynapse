//! Fuzzy-searchable GTK picker for Nerd Font glyphs, backed by `pick-icon`.

mod catalog;
mod picker;

pub use catalog::NerdIcon;
pub use picker::NerdIconPicker;
