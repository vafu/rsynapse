use zbus::zvariant::OwnedObjectPath;

/// Session bus name owned by the niri projection service.
pub const BUS_NAME: &str = "org.rsynapse.Niri";
/// ObjectManager root and root interface object path.
pub const ROOT_PATH: &str = "/org/rsynapse/Niri";
/// Root interface for service-level state and object lists.
pub const ROOT_INTERFACE: &str = "org.rsynapse.Niri1";
/// Output object interface.
pub const OUTPUT_INTERFACE: &str = "org.rsynapse.Niri1.Output";
/// Workspace object interface.
pub const WORKSPACE_INTERFACE: &str = "org.rsynapse.Niri1.Workspace";
/// Window object interface.
pub const WINDOW_INTERFACE: &str = "org.rsynapse.Niri1.Window";

/// Live D-Bus object path for an output name.
///
/// This is a live object location, not a durable identity for persisted Locus
/// relations.
pub fn output_path(name: &str) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Outputs/{}", encode_segment(name)))
}

/// Live D-Bus object path for a niri workspace id.
///
/// Workspace ids are stable while the workspace exists, but this object path is
/// still a live service address rather than a cross-session durable identity.
pub fn workspace_path(id: u64) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Workspaces/workspace_{id}"))
}

/// Live D-Bus object path for a niri window id.
///
/// Window ids are live-window scoped and must not be used as durable identity
/// after the window closes.
pub fn window_path(id: u64) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Windows/window_{id}"))
}

fn encode_segment(input: &str) -> String {
    let mut output = String::from("x");
    for byte in input.bytes() {
        output.push_str(&format!("{byte:02X}"));
    }
    output
}

fn object_path(path: impl Into<String>) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path.into()).expect("constructed D-Bus object path should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_output_names_as_single_dbus_path_segments() {
        assert_eq!(encode_segment("DP-1"), "x44502D31");
        assert_eq!(encode_segment("1-weird/name"), "x312D77656972642F6E616D65");
        assert_eq!(encode_segment(""), "x");
    }

    #[test]
    fn output_name_encoding_is_injective_for_previous_collision_cases() {
        let names = ["-", "_2D", "", "_", "DP-1", "DP_2D1", "é"];
        let mut encoded = names.into_iter().map(encode_segment).collect::<Vec<_>>();
        encoded.sort();
        encoded.dedup();
        assert_eq!(encoded.len(), names.len());
    }

    #[test]
    fn constructs_stable_object_paths() {
        assert_eq!(
            workspace_path(42).as_str(),
            "/org/rsynapse/Niri/Workspaces/workspace_42"
        );
        assert_eq!(
            window_path(7).as_str(),
            "/org/rsynapse/Niri/Windows/window_7"
        );
        assert_eq!(
            output_path("eDP-1").as_str(),
            "/org/rsynapse/Niri/Outputs/x6544502D31"
        );
    }
}
