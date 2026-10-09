#!/usr/bin/env bash
# Optional local Grafana editing integration, separate from the GTK shell install.
set -euo pipefail
repo="$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")"
prefix="${PREFIX:-$HOME/.local}"
unit_dir="$HOME/.config/systemd/user"
bash "$repo/dashboard-api/build.sh" --locked
npm --prefix "$repo/dashboard-api/grafana-plugin" ci --registry=https://registry.npmjs.org
npm --prefix "$repo/dashboard-api/grafana-plugin" run build
install -d "$prefix/bin" "$unit_dir"
systemctl --user stop rsynapse-dashboard-api.service 2>/dev/null || true
install -m 0755 "$repo/steward/target/release/rsynapse-dashboard-api" "$prefix/bin/rsynapse-dashboard-api"
sed "s|@LOCAL_BIN@|$prefix/bin|g" "$repo/install/systemd/user/rsynapse-dashboard-api.service.in" > "$unit_dir/rsynapse-dashboard-api.service"
systemctl --user daemon-reload
systemctl --user enable --now rsynapse-dashboard-api.service
docker compose -f "$repo/observability/docker-compose.yml" up -d grafana
