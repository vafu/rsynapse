//! Injective path construction for opaque IDs. Paths remain service addresses;
//! clients read the `Id` property rather than parsing or decoding a path.

use std::fmt::Write;

use zbus::zvariant::OwnedObjectPath;

use crate::ROOT_PATH;

pub fn output_path(name: &str) -> OwnedObjectPath {
    object_path("Outputs", name)
}

pub fn workspace_path(id: &str) -> OwnedObjectPath {
    object_path("Workspaces", id)
}

pub fn window_path(id: &str) -> OwnedObjectPath {
    object_path("Windows", id)
}

fn object_path(kind: &str, id: &str) -> OwnedObjectPath {
    let mut path = format!("{ROOT_PATH}/{kind}/x");
    for byte in id.bytes() {
        write!(path, "{byte:02X}").expect("writing to a String cannot fail");
    }
    OwnedObjectPath::try_from(path).expect("hex-encoded object path is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_identifiers_cannot_alias_or_escape_their_collection() {
        let ids = ["7", "007", "DP-1:1", "DP-1/1", "_2F", "/", "é", "e", ""];
        let mut paths = ids.map(workspace_path).to_vec();
        paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        paths.dedup();
        assert_eq!(paths.len(), ids.len());
        assert!(paths.iter().all(|path| {
            path.as_str()
                .strip_prefix(&format!("{ROOT_PATH}/Workspaces/"))
                .is_some_and(|segment| !segment.contains('/'))
        }));
        assert_ne!(window_path("7"), workspace_path("7"));
        assert_ne!(output_path("7"), workspace_path("7"));
    }
}
