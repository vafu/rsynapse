#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
exec cargo build --release --manifest-path "$root/Cargo.toml" "$@"
