fn main() {
    // gtk4-layer-shell must be linked before GTK/libwayland so its Wayland
    // hooks are used without relying on LD_PRELOAD service drop-ins.
    // Use the configured native prefix at runtime too (including Nix dev shells).
    // Derive paths from pkg-config, never by scanning an installation/store.
    for library in ["gtk4-layer-shell-0", "gtksourceview-5", "libadwaita-1"] {
        let native = pkg_config::Config::new()
            .probe(library)
            .unwrap_or_else(|error| panic!("native dependency {library}: {error}"));
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
            for path in native.link_paths {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
            }
        }
    }
}
