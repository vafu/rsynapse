use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackendKind {
    #[default]
    Auto,
    Niri,
    Umbriel,
}

impl FromStr for BackendKind {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "niri" => Ok(Self::Niri),
            "umbriel" => Ok(Self::Umbriel),
            _ => anyhow::bail!("unsupported backend: {value}; expected auto, niri or umbriel"),
        }
    }
}

impl BackendKind {
    pub(crate) fn resolve(self) -> anyhow::Result<Self> {
        self.select(
            std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
            std::env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty()),
            std::env::var_os("UMBRIEL_SOCKET").is_some_and(|v| !v.is_empty()),
        )
    }

    fn select(
        self,
        desktop: Option<&str>,
        niri_socket: bool,
        umbriel_socket: bool,
    ) -> anyhow::Result<Self> {
        if self != Self::Auto {
            return Ok(self);
        }
        let desktops: Vec<_> = desktop.unwrap_or_default().split(':').collect();
        // Session identity wins over inherited sockets, including nested sessions.
        let niri = desktops
            .iter()
            .any(|name| name.eq_ignore_ascii_case("niri"));
        let umbriel = desktops
            .iter()
            .any(|name| name.eq_ignore_ascii_case("umbriel"));
        match (niri, umbriel) {
            (true, false) => return Ok(Self::Niri),
            (false, true) => return Ok(Self::Umbriel),
            (true, true) => {
                anyhow::bail!("ambiguous desktop; use --backend niri or --backend umbriel")
            }
            _ => (),
        }
        match (niri_socket, umbriel_socket) {
            (true, false) => Ok(Self::Niri),
            (false, true) => Ok(Self::Umbriel),
            (true, true) => {
                anyhow::bail!("both compositor sockets are set; choose --backend explicitly")
            }
            (false, false) => {
                anyhow::bail!("cannot identify compositor; use --backend niri or --backend umbriel")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_identity_and_explicit_selection_beat_inherited_sockets() {
        assert_eq!(
            BackendKind::Auto
                .select(Some("noctalia:Umbriel"), true, false)
                .unwrap(),
            BackendKind::Umbriel
        );
        assert_eq!(
            BackendKind::Auto.select(Some("niri"), true, true).unwrap(),
            BackendKind::Niri
        );
        assert_eq!(
            BackendKind::Umbriel
                .select(Some("niri"), true, false)
                .unwrap(),
            BackendKind::Umbriel
        );
        assert_eq!(
            BackendKind::Niri.select(None, false, true).unwrap(),
            BackendKind::Niri
        );
    }
    #[test]
    fn socket_only_detection_needs_an_unambiguous_source() {
        assert_eq!(
            BackendKind::Auto.select(None, false, true).unwrap(),
            BackendKind::Umbriel
        );
        assert_eq!(
            BackendKind::Auto
                .select(Some("unknown"), true, false)
                .unwrap(),
            BackendKind::Niri
        );
        assert!(BackendKind::Auto.select(None, true, true).is_err());
        assert!(BackendKind::Auto.select(None, false, false).is_err());
        assert!(
            BackendKind::Auto
                .select(Some("niri:umbriel"), false, true)
                .is_err()
        );
        assert!("other".parse::<BackendKind>().is_err());
    }
}
