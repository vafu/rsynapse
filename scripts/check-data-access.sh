#!/bin/sh
# Enforce the app data boundary: UI widgets consume data-sources, not zbus or
# subprocesses directly. Server implementations and test fixtures are allowed.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

status=0
check_no_matches() {
    label=$1
    pattern=$2
    shift 2
    if matches=$(rg -n --glob '*.rs' "$pattern" "$@" 2>/dev/null); then
        echo "error: $label" >&2
        echo "$matches" >&2
        status=1
    fi
}

check_no_matches \
    "shell widgets may not access zbus directly; add an adapter to apps/shared/data-sources" \
    '(^|[^.[:alnum:]_])zbus::|use zbus' \
    apps/shell/src \
    --glob '!**/test.rs' \
    --glob '!**/test/**'

check_no_matches \
    "shell widgets may not spawn processes directly; add an adapter to apps/shared/data-sources" \
    'Command::new|tokio::process|std::process' \
    apps/shell/src \
    --glob '!**/test.rs' \
    --glob '!**/test/**' \
    --glob '!main.rs'

check_no_matches \
    "launcher plugins may not access zbus directly; add a blocking adapter to apps/shared/data-sources" \
    '(^|[^.[:alnum:]_])zbus::|use zbus' \
    apps/launcher/rsynapse-plugin-*

check_no_matches \
    "launcher plugins may not spawn data-collection processes directly; add an adapter to apps/shared/data-sources" \
    'Command::new|tokio::process|std::process' \
    apps/launcher/rsynapse-plugin-audio \
    apps/launcher/rsynapse-plugin-bluetooth \
    apps/launcher/rsynapse-plugin-calc \
    apps/launcher/rsynapse-plugin-workspaces

exit "$status"
