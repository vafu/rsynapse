fn main() {
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
