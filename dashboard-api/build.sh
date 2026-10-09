#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
export CARGO_HOME="$root/../steward/.cargo-home"
export CARGO_TARGET_DIR="$root/../steward/target"
mkdir -p "$CARGO_HOME"
cd /tmp
exec cargo build --release --manifest-path "$root/Cargo.toml" "$@"
