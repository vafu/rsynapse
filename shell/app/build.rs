fn main() {
    // gtk4-layer-shell must be linked before GTK/libwayland so its Wayland
    // hooks are used without relying on LD_PRELOAD service drop-ins.
    pkg_config::Config::new()
        .probe("gtk4-layer-shell-0")
        .expect("failed to find gtk4-layer-shell-0 with pkg-config");
}
