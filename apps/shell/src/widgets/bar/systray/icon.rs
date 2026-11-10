use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use data_sources::systray::{TrayIcon, TrayPixmap};
use shell_core::gtk::{self, gdk, glib};

const FALLBACK_ICON: &str = "application-x-executable-symbolic";

pub(super) fn apply(image: &gtk::Image, icon: &TrayIcon, target_size: i32) {
    match icon {
        TrayIcon::Name(name) => {
            image.set_paintable(None::<&gdk::Paintable>);
            let name = if theme_has_icon(name) {
                name.as_str()
            } else {
                FALLBACK_ICON
            };
            image.set_icon_name(Some(name));
        }
        TrayIcon::Pixmap(pixmaps) => match texture(pixmaps, target_size) {
            Some(texture) => {
                image.set_icon_name(None);
                image.set_paintable(Some(&texture));
            }
            None => apply(image, &TrayIcon::None, target_size),
        },
        TrayIcon::None => {
            image.set_paintable(None::<&gdk::Paintable>);
            image.set_icon_name(Some(FALLBACK_ICON));
        }
    }
}

pub(super) fn register_theme_path(path: &str) {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return;
    }
    static REGISTERED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    let registered = REGISTERED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut registered) = registered.lock() else {
        return;
    };
    if !registered.insert(path.clone()) {
        return;
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&path);
    }
}

pub(super) fn image(icon: &TrayIcon, size: i32) -> gtk::Image {
    let image = gtk::Image::new();
    image.set_pixel_size(size);
    apply(&image, icon, size);
    image
}

fn theme_has_icon(icon: &str) -> bool {
    gtk::gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display).has_icon(icon))
        .unwrap_or(false)
}

fn texture(pixmaps: &[TrayPixmap], target_size: i32) -> Option<gdk::MemoryTexture> {
    let pixmap = pixmaps.iter().min_by_key(|pixmap| {
        let size = i32::try_from(pixmap.width).unwrap_or(i32::MAX);
        (size - target_size).abs()
    })?;
    let (width, height, rgba) = rgba(pixmap)?;
    Some(gdk::MemoryTexture::new(
        width,
        height,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(rgba),
        (pixmap.width * 4) as usize,
    ))
}

fn rgba(pixmap: &TrayPixmap) -> Option<(i32, i32, Vec<u8>)> {
    let width = i32::try_from(pixmap.width).ok()?;
    let height = i32::try_from(pixmap.height).ok()?;
    let pixels = usize::try_from(width.checked_mul(height)?).ok()?;
    let expected = pixels.checked_mul(4)?;
    if pixmap.argb.len() < expected {
        return None;
    }

    let mut rgba = Vec::with_capacity(expected);
    for pixel in pixmap.argb.chunks_exact(4).take(pixels) {
        let [alpha, red, green, blue] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        rgba.extend_from_slice(&[red, green, blue, alpha]);
    }
    Some((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::rgba;
    use data_sources::systray::TrayPixmap;

    #[test]
    fn converts_argb_pixmap_to_rgba() {
        let pixmap = TrayPixmap {
            width: 1,
            height: 1,
            argb: vec![0x80, 0x10, 0x20, 0x30],
        };
        let (width, height, rgba) = rgba(&pixmap).expect("rgba");
        assert_eq!((width, height), (1, 1));
        assert_eq!(rgba, vec![0x10, 0x20, 0x30, 0x80]);
    }

    #[test]
    fn rejects_short_pixmap() {
        let pixmap = TrayPixmap {
            width: 2,
            height: 2,
            argb: vec![0, 0, 0, 0],
        };
        assert!(rgba(&pixmap).is_none());
    }
}
