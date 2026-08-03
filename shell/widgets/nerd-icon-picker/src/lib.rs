//! Searchable GTK picker for Nerd Font glyphs.

mod catalog;
mod picker;

pub use catalog::{NerdIcon, search_icons};
pub use picker::NerdIconPicker;
