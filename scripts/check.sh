#!/usr/bin/env bash
# Repository-wide verification entry point.
#
# Usage:
#   scripts/check.sh <fmt|clippy|test|test-dbus|test-install|test-python|test-panel|manifest|all> [--live]
#
# The default gate is hermetic. Live compositor/GTK tests are opt-in with
# --live. Cargo artifacts go to CARGO_TARGET_DIR when set; otherwise a temporary
# shared directory is used so the source tree is not polluted.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    export CARGO_TARGET_DIR="/tmp/rsynapse-check-target"
fi

WORKSPACES=(
    dashboard-api
    locus
    projd
    shell
    shell/launcher
    steward
    workspace-dbus
)

LIVE=0
MODE=""
for arg in "$@"; do
    case "$arg" in
        --live) LIVE=1 ;;
        -h|--help)
            sed -n '2,10p' "$0"
            exit 0
            ;;
        *) MODE="$arg" ;;
    esac
done

if [[ -z "$MODE" ]]; then
    echo "usage: $0 <fmt|clippy|test|test-dbus|test-install|test-python|test-panel|manifest|all> [--live]" >&2
    exit 2
fi

FAILED=0
FAILURES=()

run_step() {
    local name="$1"
    shift
    printf '\n=== %s ===\n' "$name"
    if "$@"; then
        printf 'PASS: %s\n' "$name"
    else
        printf 'FAIL: %s\n' "$name"
        FAILED=1
        FAILURES+=("$name")
    fi
}

cargo_workspace() {
    local command="$1"
    local workspace="$2"
    shift 2
    echo "--- $workspace ($command) ---"
    if [[ "$command" == "fmt" ]]; then
        cargo fmt --manifest-path "$workspace/Cargo.toml" "$@"
    else
        cargo "$command" --manifest-path "$workspace/Cargo.toml" --workspace "$@"
    fi
    local status=$?
    if [[ $status -ne 0 ]]; then
        echo "workspace failed: $workspace ($command)" >&2
    fi
    return $status
}

run_fmt() {
    local workspace
    for workspace in "${WORKSPACES[@]}"; do
        cargo_workspace fmt "$workspace" --all --check || return 1
    done
}

run_clippy() {
    local workspace
    # Keep the baseline visible: do not promote warnings to errors yet.
    for workspace in "${WORKSPACES[@]}"; do
        cargo_workspace clippy "$workspace" --all-targets || return 1
    done
}

run_test() {
    local workspace
    for workspace in "${WORKSPACES[@]}"; do
        cargo_workspace test "$workspace" || return 1
    done
}

run_manifest() {
    local status=0

    echo "--- no manifest path may escape the repository ---"
    local offenders
    offenders="$(python3 - "$ROOT" <<'PY'
import pathlib, re, sys

root = pathlib.Path(sys.argv[1]).resolve()
pattern = re.compile(r'path\s*=\s*"([^"]+)"')
for manifest in sorted(root.rglob("Cargo.toml")):
    parts = set(manifest.parts)
    if "target" in parts or ".cargo-home" in parts or ".git" in parts:
        continue
    try:
        text = manifest.read_text()
    except OSError:
        continue
    for match in pattern.finditer(text):
        candidate = (manifest.parent / match.group(1)).resolve()
        if root not in candidate.parents and candidate != root:
            print(f"{manifest.relative_to(root)}: {match.group(1)}")
PY
)"
    if [[ -n "$offenders" ]]; then
        echo "$offenders"
        echo "manifest paths escape the repository" >&2
        status=1
    fi

    echo "--- library-only stray lockfiles ---"
    local lock
    while IFS= read -r lock; do
        case "$lock" in
            ./dashboard-api/Cargo.lock|./locus/Cargo.lock|./projd/Cargo.lock|./shell/Cargo.lock|./shell/launcher/Cargo.lock|./steward/Cargo.lock|./workspace-dbus/Cargo.lock) ;;
            *)
                if [[ "$lock" != *"/target/"* && "$lock" != *"/.cargo-home/"* ]]; then
                    echo "unexpected lockfile: ${lock#./}" >&2
                    status=1
                fi
                ;;
        esac
    done < <(find . -name Cargo.lock -not -path '*/target/*' -not -path './.git/*' | sort)
    return $status
}

run_install_tests() {
    python3 -m unittest discover -s install/tests -p 'test_*.py'
}

run_python_syntax() {
    python3 -m compileall -q install projd/tests steward/tests dashboard-api/tests \
        workspace-dbus/service/tests
}

build_release_bins() {
    # Build the binaries used by the private-session integration tests.
    cargo build --release --manifest-path projd/Cargo.toml --workspace \
        && cargo build --release --manifest-path steward/Cargo.toml --workspace \
        && cargo build --release --manifest-path locus/Cargo.toml --workspace \
        && cargo build --release --manifest-path workspace-dbus/Cargo.toml --workspace \
        && cargo build --release --manifest-path dashboard-api/Cargo.toml --workspace
}

run_dbus_tests() {
    build_release_bins || return 1

    local status=0
    local tests=(
        projd/tests/integration.py
        projd/tests/checklist.py
        projd/tests/labels.py
        projd/tests/metrics.py
        projd/tests/cancellation.py
        dashboard-api/tests/metrics.py
    )
    local test
    for test in "${tests[@]}"; do
        echo "--- dbus-run-session: $test ---"
        if ! dbus-run-session -- python3 "$test"; then
            status=1
        fi
    done

    echo "--- dbus-run-session: shell-source systray ---"
    if ! dbus-run-session -- cargo test --manifest-path shell/Cargo.toml \
        -p shell-source --test systray -- --ignored; then
        status=1
    fi

    if [[ $LIVE -eq 1 ]]; then
        echo "--- live tests ---"
        # These require a live compositor/session and are never part of the
        # hermetic gate.
        dbus-run-session -- python3 workspace-dbus/service/tests/umbriel_live.py || status=1
        dbus-run-session -- python3 dashboard-api/tests/integration.py || status=1
        artifacts="$(mktemp -d)"
        dbus-run-session -- python3 steward/tests/session_restore.py --artifacts "$artifacts" || status=1
        rm -rf "$artifacts"
    fi
    return $status
}

run_panel_tests() {
    (
        cd dashboard-api/grafana-plugin
        npm ci --no-audit --no-fund
        npm run typecheck
        npm run build
        npm test
    )
}

case "$MODE" in
    fmt) run_step fmt run_fmt ;;
    clippy) run_step clippy run_clippy ;;
    test) run_step test run_test ;;
    test-dbus) run_step test-dbus run_dbus_tests ;;
    test-install) run_step test-install run_install_tests ;;
    test-python) run_step test-python run_python_syntax ;;
    test-panel) run_step test-panel run_panel_tests ;;
    manifest) run_step manifest run_manifest ;;
    all)
        run_step manifest run_manifest
        run_step fmt run_fmt
        run_step test run_test
        run_step test-install run_install_tests
        run_step test-python run_python_syntax
        run_step test-panel run_panel_tests
        run_step clippy run_clippy
        run_step test-dbus run_dbus_tests
        ;;
    *)
        echo "unknown mode: $MODE" >&2
        exit 2
        ;;
esac

if [[ $FAILED -ne 0 ]]; then
    printf '\nFAILED steps:\n' >&2
    printf '  - %s\n' "${FAILURES[@]}" >&2
    exit 1
fi

echo
echo "All selected checks passed."
