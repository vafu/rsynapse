#!/usr/bin/env bash
# Build the shell in-place and restart its units.
# The live binaries are run through home-manager wrappers in ~/.local/bin,
# so do not `cargo install` into ~/.local and overwrite those wrappers.
# Use `nix develop` at the repository root for Nix-provided GTK dependencies.
set -euo pipefail

cargo build --release -p rsynapse-shell "$@"
systemctl --user restart rsynapse-shell.service rsynapse-notifications.service
systemctl --user is-active rsynapse-shell.service rsynapse-notifications.service
