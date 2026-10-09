#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
export CARGO_HOME="$(realpath "$root/../steward/.cargo-home")"
export CARGO_TARGET_DIR="$(realpath "$root/../steward/target")"
cd /tmp
exec cargo build --release --manifest-path "$root/Cargo.toml" "$@"
