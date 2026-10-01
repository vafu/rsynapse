#!/usr/bin/env bash
# Builds rsynapse-metrics against the PUBLIC crates.io registry.
#
# Background: the user-wide ~/.cargo/config.toml routes crates.io through
# an internal mirror, and cargo discovers that config by walking up from
# the working directory -- so any cargo run under $HOME would use it.
# This wrapper instead runs cargo from a neutral CWD (/tmp) with an
# isolated CARGO_HOME (metrics/.cargo-home), where no such config exists
# and plain crates.io applies. Direct `cargo` runs inside metrics/ fail
# loudly via metrics/.cargo/config.toml; always use this script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_HOME="$SCRIPT_DIR/.cargo-home"
export CARGO_TARGET_DIR="$SCRIPT_DIR/target"

mkdir -p "$CARGO_HOME"
cd /tmp
exec cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" "$@"
