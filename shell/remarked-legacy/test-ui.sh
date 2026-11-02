#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")/../..")"
export PROJD_BIN="${PROJD_BIN:-$(command -v rsynapse-projd)}"
export REMARKED_BIN="${REMARKED_BIN:-$root/../remarked/target/debug/remarked}"
export REMARKED_CLI_BIN="${REMARKED_CLI_BIN:-$root/../remarked/target/debug/remarked-cli}"
# These must be inherited by the private bus daemon's activation environment,
# not just the GTK test. In particular, a GTK3 portal must not start a second
# accessibility bridge against the live display's shared AT-SPI socket.
export NO_AT_BRIDGE=1 GTK_A11Y=none GIO_USE_VFS=local GSK_RENDERER=cairo RSYNAPSE_UI_TEST=1
if [[ "${RSYNAPSE_UI_LIVE_DISPLAY:-}" == 1 ]]; then
  exec dbus-run-session -- cargo test \
    --manifest-path "$root/shell/Cargo.toml" -p rsynapse-workspace \
    workspace_ui_roundtrip -- --ignored --nocapture --test-threads=1
fi
exec dbus-run-session -- python3 "$root/shell/remarked/headless-ui.py" "$root"
