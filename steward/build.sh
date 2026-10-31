#!/usr/bin/env bash
# Build with the caller's normal Cargo configuration and environment.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" "$@"
