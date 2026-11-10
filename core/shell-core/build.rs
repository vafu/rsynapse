fn main() {
    for library in [
        "gtk4",
        "gtk4-layer-shell-0",
        "glib-2.0",
        "gdk-pixbuf-2.0",
        "pango",
        "cairo",
        "graphene-1.0",
    ] {
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
