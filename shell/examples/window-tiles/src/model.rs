use shell_core::source::Source;

use crate::niri::NiriWindow;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct WindowTile {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    focused: bool,
}

impl WindowTile {
    pub(crate) fn source(window: NiriWindow) -> Source<Self> {
        window
            .id()
            .combine_latest(window.title(), |id, title| (id, title))
            .combine_latest(window.app_id(), |(id, title), app_id| (id, title, app_id))
            .combine_latest(window.focused(), |(id, title, app_id), focused| {
                Self::from_dbus(id, title, app_id, focused)
            })
    }

    pub(crate) fn from_dbus(
        id: u64,
        title: Option<String>,
        app_id: Option<String>,
        focused: bool,
    ) -> Self {
        Self {
            id,
            title,
            app_id,
            focused,
        }
    }

    pub(crate) fn classes(&self) -> Vec<&'static str> {
        if self.focused {
            vec!["window-tile", "window-tile-focused"]
        } else {
            vec!["window-tile"]
        }
    }

    pub(crate) fn icon_name(&self) -> &str {
        self.app_id
            .as_deref()
            .filter(|app_id| !app_id.is_empty())
            .unwrap_or("application-x-executable")
    }

    pub(crate) fn title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|title| !title.is_empty())
            .or_else(|| self.app_id.as_deref().filter(|app_id| !app_id.is_empty()))
            .unwrap_or("Window")
    }
}

#[cfg(test)]
mod tests {
    use super::WindowTile;

    #[test]
    fn uses_app_id_as_icon_name_and_title_fallback() {
        let tile = WindowTile::from_dbus(9, None, Some("firefox".to_owned()), false);

        assert_eq!(tile.title(), "firefox");
        assert_eq!(tile.icon_name(), "firefox");
    }

    #[test]
    fn ignores_empty_optional_dbus_values() {
        let tile = WindowTile::from_dbus(9, Some("".to_owned()), Some("".to_owned()), false);

        assert_eq!(tile.title(), "Window");
        assert_eq!(tile.icon_name(), "application-x-executable");
    }
}
