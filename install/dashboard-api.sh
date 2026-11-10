#!/usr/bin/env bash
# Optional local Grafana metrics/settings integration, separate from the GTK shell install.
set -euo pipefail
repo="$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")"
prefix="${PREFIX:-$HOME/.local}"
config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
unit_dir="$config_home/systemd/user"
npm --prefix "$repo/misc/dashboard-api/grafana-plugin" ci
npm --prefix "$repo/misc/dashboard-api/grafana-plugin" run build
install -d "$prefix/bin" "$unit_dir"
cargo install --path "$repo/misc/dashboard-api" --locked --force --root "$prefix"
systemctl --user stop rsynapse-dashboard-api.service 2>/dev/null || true
sed "s|@LOCAL_BIN@|$prefix/bin|g" "$repo/install/systemd/user/rsynapse-dashboard-api.service.in" > "$unit_dir/rsynapse-dashboard-api.service"
systemctl --user daemon-reload
systemctl --user enable --now rsynapse-dashboard-api.service
docker compose -f "$repo/misc/observability/docker-compose.yml" up -d grafana
