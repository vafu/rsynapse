#!/usr/bin/env bash
# Build/install the shell with the caller's environment, then restart its units.
# Use `nix develop` at the repository root for Nix-provided GTK dependencies.
set -euo pipefail

shell_dir="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
prefix="${PREFIX:-$HOME/.local}"

cargo install --path "$shell_dir/app" --locked --force --root "$prefix" "$@"
systemctl --user restart rsynapse-shell.service rsynapse-notifications.service
systemctl --user is-active rsynapse-shell.service rsynapse-notifications.service
