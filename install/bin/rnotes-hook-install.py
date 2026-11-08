#!/usr/bin/env python3
"""Register the rnotes SessionStart hook and persist its trust hash.

Codex gates hooks on a per-handler trust hash stored in config.toml. Editing
hooks.json does not require re-trusting unrelated hooks; each handler has its own
`hooks.state` entry keyed by file, event, group index, and handler index. This
script adds the hook (idempotently), asks the app server for the authoritative
`currentHash` of the new handler, and writes the matching trust entry.
"""
import json
import pathlib
import subprocess
import sys
import time

CODEX_HOME = pathlib.Path.home() / ".codex"
HOOKS = CODEX_HOME / "hooks.json"
CONFIG = CODEX_HOME / "config.toml"


def load(path):
    return json.loads(path.read_text())


def add_hook(command):
    data = load(HOOKS)
    groups = data.setdefault("hooks", {}).setdefault("SessionStart", [])
    for group in groups:
        for handler in group.get("hooks", []):
            if handler.get("command") == command:
                return data
    groups.append({"hooks": [{"type": "command", "command": command}]})
    HOOKS.write_text(json.dumps(data, indent=2) + "\n")
    return data


def handler_key(command):
    data = load(HOOKS)
    for index, group in enumerate(data["hooks"]["SessionStart"]):
        for handler_index, handler in enumerate(group.get("hooks", [])):
            if handler.get("command") == command:
                return f"{HOOKS}:session_start:{index}:{handler_index}"
    raise SystemExit(f"hook not found after install: {command}")


def query_hash(key):
    messages = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "rnotes-hook-install", "version": "0"}},
        },
        {"jsonrpc": "2.0", "method": "initialized", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "hooks/list", "params": {"cwds": [str(pathlib.Path.cwd())]}},
    ]
    process = subprocess.Popen(
        ["codex", "app-server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )

    def send(message):
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

    def wait_for(request_id, timeout=20):
        deadline = time.time() + timeout
        while time.time() < deadline:
            line = process.stdout.readline()
            if not line:
                return None
            try:
                response = json.loads(line)
            except ValueError:
                continue
            if response.get("id") == request_id:
                return response
        return None

    try:
        send(messages[0])
        wait_for(1)
        send(messages[1])
        time.sleep(0.2)
        send(messages[2])
        response = wait_for(2)
    finally:
        process.terminate()
    if not response:
        raise SystemExit("hooks/list did not respond; cannot determine hook hash")
    for entry in response["result"]["data"]:
        for hook in entry["hooks"]:
            if hook["key"] == key:
                return hook["currentHash"]
    raise SystemExit(f"hook {key} not reported by hooks/list")


def write_trust(key, current_hash):
    text = CONFIG.read_text()
    header = f'[hooks.state."{key}"]'
    if header in text:
        print(f"trust entry already present: {key}")
        return
    entry = f'\n{header}\ntrusted_hash = "{current_hash}"\n'
    # Insert at the end of the [hooks.state] section so the key stays inside it.
    section = "[hooks.state]"
    if section not in text:
        text = text.rstrip() + f"\n\n{section}\n"
    start = text.index(section)
    next_header = text.find("\n[", start + len(section))
    if next_header == -1:
        text = text.rstrip() + "\n" + entry
    else:
        text = text[:next_header] + entry + text[next_header:]
    CONFIG.write_text(text)
    print(f"trusted hook: {key}")


def main():
    command = str(pathlib.Path.home() / ".local/bin/rnotes-session-context")
    add_hook(command)
    key = handler_key(command)
    current_hash = query_hash(key)
    write_trust(key, current_hash)


if __name__ == "__main__":
    sys.exit(main())
