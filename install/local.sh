#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="${PREFIX:-"$HOME/.local"}"
local_bin="$prefix/bin"
dbus_dir="$prefix/share/dbus-1/services"
plugin_dir="$prefix/lib/rsynapse/plugins"
systemd_user_dir="$HOME/.config/systemd/user"

cargo_install() {
    local path="$1"
    cargo install --path "$path" --locked --force --root "$prefix"
}

install_templates() {
    local source_dir="$1"
    local target_dir="$2"
    local suffix="$3"

    install -d "$target_dir"
    for template in "$source_dir"/*"$suffix"; do
        local name
        name="$(basename "$template" "$suffix")"
        sed "s|@LOCAL_BIN@|$local_bin|g" "$template" > "$target_dir/$name"
        chmod 0644 "$target_dir/$name"
    done
}

echo "Installing release binaries to $local_bin"
cargo_install "$repo_root/locus"
cargo_install "$repo_root/niri-dbus"
cargo_install "$repo_root/shell/app"
cargo_install "$repo_root/shell/launcher/rsynapse-daemon"
cargo_install "$repo_root/shell/launcher/rsynapse-cli"
cargo_install "$repo_root/shell/launcher/rsynapse-ui"

echo "Installing helper scripts to $local_bin"
install -d "$local_bin"
install -m 0755 "$repo_root/install/bin/proj" "$local_bin/proj"

echo "Building and installing launcher plugins to $plugin_dir"
cargo build \
    --manifest-path "$repo_root/shell/launcher/Cargo.toml" \
    --release \
    -p rsynapse-plugin-launcher \
    -p rsynapse-plugin-shell \
    -p rsynapse-plugin-calc \
    -p rsynapse-plugin-commands

install -d "$plugin_dir"
plugin_target="${CARGO_TARGET_DIR:-"$repo_root/shell/launcher/target"}/release"
find "$plugin_target" -maxdepth 1 -type f -name 'librsynapse_plugin_*.so' \
    -exec install -m 0755 {} "$plugin_dir/" \;

echo "Installing D-Bus activation files to $dbus_dir"
install_templates "$repo_root/install/dbus-1/services" "$dbus_dir" ".in"

echo "Installing systemd user units to $systemd_user_dir"
install_templates "$repo_root/install/systemd/user" "$systemd_user_dir" ".in"

echo "Removing deprecated Rsynapse service definitions"
rm -f \
    "$dbus_dir/com.rsynapse.Launcher.service" \
    "$dbus_dir/com.rsynapse.Engine.service" \
    "$dbus_dir/io.github.rsynapse.Locus.service" \
    "$dbus_dir/io.github.rsynapse.Niri.service" \
    "$dbus_dir/org.rsynapse.Launcher.service" \
    "$dbus_dir/org.rsynapse.UI.service" \
    "$systemd_user_dir/locus.service" \
    "$systemd_user_dir/locusfs.service" \
    "$systemd_user_dir/niri-dbus.service" \
    "$systemd_user_dir/rsynapse-daemon.service" \
    "$systemd_user_dir/rsynapse-ui.service" \
    "$local_bin/locusfs"

rm -f \
    "$systemd_user_dir/default.target.wants/locus.service" \
    "$systemd_user_dir/default.target.wants/locusfs.service" \
    "$systemd_user_dir/default.target.wants/rsynapse-daemon.service" \
    "$systemd_user_dir/default.target.wants/rsynapse-ui.service" \
    "$systemd_user_dir/graphical-session.target.wants/niri-dbus.service"

rm -rf \
    "$systemd_user_dir/rsynapse-shell.service.d" \
    "$systemd_user_dir/rsynapse-notifications.service.d"

if systemctl --user daemon-reload >/dev/null 2>&1; then
    systemctl --user enable rsynapse-shell.service rsynapse-notifications.service
    systemctl --user reset-failed rsynapse-shell.service rsynapse-notifications.service >/dev/null 2>&1 || true
else
    echo "systemd user manager is not available; skipped daemon-reload and enable"
fi

echo "Rsynapse local install complete"
