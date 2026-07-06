use zbus::zvariant::OwnedObjectPath;

pub const BUS_NAME: &str = "org.rsynapse.Niri";
pub const ROOT_PATH: &str = "/org/rsynapse/Niri";
#[allow(dead_code)]
pub const ROOT_INTERFACE: &str = "org.rsynapse.Niri1";
#[allow(dead_code)]
pub const OUTPUT_INTERFACE: &str = "org.rsynapse.Niri1.Output";
#[allow(dead_code)]
pub const WORKSPACE_INTERFACE: &str = "org.rsynapse.Niri1.Workspace";
#[allow(dead_code)]
pub const WINDOW_INTERFACE: &str = "org.rsynapse.Niri1.Window";

pub fn output_path(name: &str) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Outputs/{}", encode_segment(name)))
}

pub fn workspace_path(id: u64) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Workspaces/workspace_{id}"))
}

pub fn window_path(id: u64) -> OwnedObjectPath {
    object_path(format!("{ROOT_PATH}/Windows/window_{id}"))
}

pub fn optional_path(path: Option<OwnedObjectPath>) -> Vec<OwnedObjectPath> {
    path.into_iter().collect()
}

pub fn encode_segment(input: &str) -> String {
    let mut output = String::new();
    for (index, byte) in input.bytes().enumerate() {
        let valid = byte.is_ascii_alphanumeric() || byte == b'_';
        let valid_first = byte.is_ascii_alphabetic() || byte == b'_';
        if valid && (index > 0 || valid_first) {
            output.push(byte as char);
        } else {
            output.push('_');
            output.push_str(&format!("{byte:02X}"));
        }
    }

    if output.is_empty() {
        "_".to_owned()
    } else {
        output
    }
}

fn object_path(path: impl Into<String>) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path.into()).expect("constructed D-Bus object path should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_output_names_as_single_dbus_path_segments() {
        assert_eq!(encode_segment("DP-1"), "DP_2D1");
        assert_eq!(encode_segment("1-weird/name"), "_31_2Dweird_2Fname");
        assert_eq!(encode_segment(""), "_");
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
            "/org/rsynapse/Niri/Outputs/eDP_2D1"
        );
    }
}
