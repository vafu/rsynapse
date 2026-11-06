#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="${PREFIX:-"$HOME/.local"}"
config_home="${XDG_CONFIG_HOME:-"$HOME/.config"}"
data_home="${XDG_DATA_HOME:-"$HOME/.local/share"}"
state_home="${XDG_STATE_HOME:-"$HOME/.local/state"}"
local_bin="$prefix/bin"
dbus_dir="$prefix/share/dbus-1/services"
applications_dir="$prefix/share/applications"
script_dir="$config_home/scripts"
systemd_user_dir="$config_home/systemd/user"

apply_policy=0
for arg in "$@"; do
    case "$arg" in
        --apply-policy) apply_policy=1 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

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
        sed -e "s|@LOCAL_BIN@|$local_bin|g" -e "s|@SCRIPT_DIR@|$script_dir|g" "$template" > "$target_dir/$name"
        chmod 0644 "$target_dir/$name"
    done
}

echo "Installing release binaries to $local_bin"
cargo_install "$repo_root/locus"
cargo_install "$repo_root/workspace-dbus/service"
cargo_install "$repo_root/workspace-dbus/terminal-id"
cargo_install "$repo_root/projd"
install -d "$local_bin"
cargo_install "$repo_root/shell/app"
cargo_install "$repo_root/shell/launcher/rsynapse-daemon"
cargo_install "$repo_root/shell/launcher/rsynapse-cli"
cargo_install "$repo_root/shell/launcher/rsynapse-ui"
cargo_install "$repo_root/shell/core/desktop-entry"

cargo_install "$repo_root/steward"

echo "Installing helper scripts to $local_bin"
install -d "$local_bin" "$script_dir"
install -m 0755 "$repo_root/install/bin/rsynapse-open-url" "$script_dir/rsynapse-open-url"
install -m 0755 "$repo_root/install/bin/remarked-ui" "$local_bin/remarked-ui"
ln -sfn "$script_dir/rsynapse-open-url" "$local_bin/rsynapse-open-url"
install -m 0755 "$repo_root/install/bin/ghostty-project-window" "$script_dir/ghostty-project-window"
ln -sfn "$script_dir/ghostty-project-window" "$local_bin/ghostty-project-window"
install -m 0755 "$repo_root/install/bin/umbriel-workspace-new" "$script_dir/umbriel-workspace-new"
ln -sfn "$script_dir/umbriel-workspace-new" "$local_bin/umbriel-workspace-new"
install -m 0755 "$repo_root/scripts/sone-cli" "$local_bin/sone-cli"

# Launcher plugins are compiled into rsynapse-daemon (static registration);
# no plugin .so files are installed.
echo "Installing desktop entries to $applications_dir"
install_templates "$repo_root/install/applications" "$applications_dir" ".in"
if [[ $apply_policy -eq 1 ]]; then
    if command -v xdg-mime >/dev/null 2>&1; then
        xdg-mime default rsynapse-open-url.desktop x-scheme-handler/http
        xdg-mime default rsynapse-open-url.desktop x-scheme-handler/https
    else
        echo "xdg-mime is not available; skipped URL handler activation"
    fi
else
    echo "Skipped MIME policy changes (pass --apply-policy to apply them)"
fi

echo "Installing namespaced daemons, systemd units and D-Bus activation files"
python3 "$repo_root/install/session-services.py" --prefix "$prefix" \
    --config-home "$config_home"

echo "Removing deprecated Rsynapse service definitions"
rm -f \
    "$dbus_dir/com.rsynapse.Launcher.service" \
    "$dbus_dir/com.rsynapse.Engine.service" \
    "$dbus_dir/io.github.rsynapse.Locus.service" \
    "$dbus_dir/io.github.rsynapse.Niri.service" \
    "$dbus_dir/org.rsynapse.Niri.service" \
    "$dbus_dir/org.rsynapse.Launcher.service" \
    "$dbus_dir/org.rsynapse.UI.service" \
    "$systemd_user_dir/locus.service" \
    "$systemd_user_dir/locusfs.service" \
    "$systemd_user_dir/niri-dbus.service" \
    "$systemd_user_dir/rsynapse-daemon.service" \
    "$systemd_user_dir/rsynapse-ui.service" \
    "$local_bin/locusfs" \
    "$local_bin/niri-dbus"

rm -f \
    "$systemd_user_dir/default.target.wants/locus.service" \
    "$systemd_user_dir/default.target.wants/locusfs.service" \
    "$systemd_user_dir/default.target.wants/rsynapse-daemon.service" \
    "$systemd_user_dir/default.target.wants/rsynapse-ui.service" \
    "$systemd_user_dir/graphical-session.target.wants/niri-dbus.service"

if [[ $apply_policy -eq 1 ]]; then
    rm -rf \
        "$systemd_user_dir/rsynapse-shell.service.d" \
        "$systemd_user_dir/rsynapse-notifications.service.d"
else
    echo "Preserved service drop-ins (pass --apply-policy to remove obsolete ones)"
fi

# Cleanup above may have removed obsolete loaded units.
systemctl --user daemon-reload >/dev/null 2>&1 || true

echo "Rsynapse local install complete"
