#!/usr/bin/env bash
# Rebuilds the rsynapse-shell release binaries and restarts the user services.
#
# Binaries land in <repo>/shell/target/release, which is exactly what the
# home-manager wrappers execute -- no separate install step. Always build
# here (never CARGO_TARGET_DIR) when the running shell should pick it up.
#
# System gtk4 (4.14) is too old for the gtk4-rs bindings (>= 4.21), so this
# assembles a pkg-config path from nix store dev outputs, preferring the
# revisions pinned by the home-manager wrapper. A local stub covers
# sysprof-capture-4, which only exists as private glib build metadata.
set -euo pipefail

SHELL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

STUB_DIR="${TMPDIR:-/tmp}/rsynapse-pkgconfig-stub"
mkdir -p "$STUB_DIR"
if [ ! -f "$STUB_DIR/sysprof-capture-4.pc" ]; then
    cat > "$STUB_DIR/sysprof-capture-4.pc" <<'EOF'
prefix=/tmp/stub-sysprof
libdir=${prefix}/lib
includedir=${prefix}/include

Name: sysprof-capture-4
Description: Local stub for private glib build metadata
Version: 4.0
Libs:
Cflags:
EOF
fi

preferred_dev() {
    find /nix/store -maxdepth 3 -type d -name pkgconfig -path "*-dev/*" 2>/dev/null \
        | grep -E "$1" | head -n 1
}

PREFERRED=(
    "gtk4-4.22.4-dev"
    "glib-2.88.3-dev"
    "pango-1.57.1-dev"
    "cairo-1.18.4-dev"
    "gdk-pixbuf-2.44"
    "graphene-1.10.8-dev"
    "libadwaita-without-adwaita-1.9.3-dev"
    "gtk4-layer-shell-1.3.0-dev"
    "harfbuzz-13.2.1-dev"
    "fribidi-1.0.16-dev"
    "libepoxy-1.5.10-dev"
    "wayland-1.26.0-dev"
    "sysprof-50.0-dev"
)

PKG_CONFIG_PATH="$STUB_DIR"
for want in "${PREFERRED[@]}"; do
    dir="$(preferred_dev "$want")"
    if [ -n "$dir" ]; then
        PKG_CONFIG_PATH="$PKG_CONFIG_PATH:$dir"
    fi
done
PKG_CONFIG_PATH="$PKG_CONFIG_PATH:$(find /nix/store -maxdepth 3 -type d -name pkgconfig -path '*-dev/*' 2>/dev/null | tr '\n' ':')"
export PKG_CONFIG_PATH

pkg-config --modversion gtk4

cargo build --release --manifest-path "$SHELL_DIR/Cargo.toml" -p rsynapse-shell

systemctl --user restart rsynapse-shell.service rsynapse-notifications.service
systemctl --user is-active rsynapse-shell.service rsynapse-notifications.service
