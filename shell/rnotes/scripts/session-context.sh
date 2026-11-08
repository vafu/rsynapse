#!/bin/sh
# Inject a short rnotes pointer at session start when the working tree has a
# Remarked notebook. Silent no-op everywhere else.
set -eu

command -v rnotes >/dev/null 2>&1 || exit 0

cwd=${PWD:-}
[ -n "$cwd" ] || exit 0

# Walk up looking for a project notebook; rnotes resolves projects the same way.
dir=$cwd
found=0
while [ -n "$dir" ]; do
    if [ -d "$dir/.notes/.zk" ]; then found=1; break; fi
    parent=$(dirname "$dir")
    [ "$parent" = "$dir" ] && break
    dir=$parent
done
[ "$found" -eq 1 ] || exit 0

python3 - "$dir" <<'PY'
import json, sys
root = sys.argv[1]
context = (
    f"This project has an rnotes task/notes notebook at {root}/.notes. "
    "Use the rnotes skill for structured work: `rnotes task list`, "
    "`rnotes task note add --role decision|progress|evidence`, and "
    "`rnotes task note check PATH` after editing task-note frontmatter."
)
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": context,
    }
}))
PY
