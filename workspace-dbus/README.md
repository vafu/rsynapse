# workspace-dbus

Compositor-independent session D-Bus projection of outputs, workspaces, windows
and focus.

The architecture is:

```text
Niri implementation ────┐
                       ├── workspace-dbus API ── shell / steward / other clients
Umbriel implementation ┘
```

## Layout and current stage

- `api/` (`workspace-dbus-api`): the standalone wire contract, typed async zbus
  client proxies, interface constants and opaque-ID path construction. No
  compositor, shell, steward, Locus or project-management dependencies.
- `service/` (`workspace-dbus`): the projection host, neutral state/object
  lifecycle and private `backend/niri` and `backend/umbriel` implementations.
  No Locus or projd dependency.
- [CONTRACT.md](CONTRACT.md): property signatures, identity/focus/lifecycle
  semantics, ownership and cutover rules.

Both implementations and shell/steward consumers use this contract.
Consumer compositor state must
depend only on `workspace-dbus-api`, never on a service implementation crate.

Projects/goals remain owned by projd. Steward decides associations and measures
work; Locus stores relations. This service does not interpret or clear those
relations. It reports object lifetime through D-Bus; consumers own relation
cleanup and persistence decisions.

## Verification

```sh
cargo test --manifest-path workspace-dbus/Cargo.toml --workspace
cargo fmt --manifest-path workspace-dbus/Cargo.toml --all --check
```

Run the current backend with:

```sh
cargo run --manifest-path workspace-dbus/Cargo.toml -p workspace-dbus -- --backend niri
```

Use `--backend umbriel` for Umbriel. The default is `--backend auto`:

- A recognized `XDG_CURRENT_DESKTOP` (`niri`/`umbriel`, case-insensitive colon
  separated desktop names) wins over inherited compositor socket variables.
- Otherwise exactly one of `NIRI_SOCKET`/`UMBRIEL_SOCKET` selects its backend.
  Ambiguous or unidentified sessions require an explicit backend.
- Selection is fixed for the process; a reconnect never silently switches
  compositor identity. `--socket PATH` overrides socket discovery for the
  explicitly selected/detected backend, not the protocol selection itself.

Niri uses `NIRI_SOCKET`. Umbriel uses `UMBRIEL_SOCKET`, falling back to
`$XDG_RUNTIME_DIR/umbriel-$WAYLAND_DISPLAY.sock`. The adapter subscribes once to
outputs/workspaces/windows/layers/overview; full family snapshots replace their
previous state. Bootstrap waits for the subscribed inventories and focus state,
not keyboards or timers. Layers and overview normalize keyboard focus without
adding backend-specific properties to the API. Native output membership is kept
independent of workspace membership for scratchpads and pinned windows.

The installer builds `service/`, installs the neutral D-Bus activation file and
`workspace-dbus.service`, and enables it under `graphical-session.target`.
Activation delegates to that unit with `--backend auto`; `PartOf` stops the
projection with the graphical session so the next compositor session selects
its own backend after environment import. On systems without systemd, the D-Bus
activation Exec still names the same binary. Use an explicit backend and a
private D-Bus when testing nested compositors alongside the current desktop.

The original Niri-to-neutral API migration requires coordinated producer,
Locus, steward and shell deployment; adding Umbriel does not change that API.

The API wire and both socket integration tests each start their own
`dbus-run-session`. They never claim the API name on the user's real session bus.
The Niri test covers bootstrap ordering, live title updates, output moves,
unresolved references, workspace removal and transport reconnection.

Umbriel tests cover full-snapshot replacement, arbitrary frame order/fragmentation,
opaque IDs, named versus anonymous numeric workspace names, global versus
remembered/pointer focus, layers, overview, workspace-less scratchpads, missing
referents, subscription errors and reconnects. An opt-in real compositor test
uses Umbriel plus Python GI/GTK4 on a private headless runtime and D-Bus:

```sh
cargo build --manifest-path workspace-dbus/Cargo.toml -p workspace-dbus
dbus-run-session -- python3 workspace-dbus/service/tests/umbriel_live.py \
  --producer "$PWD/workspace-dbus/target/debug/workspace-dbus" \
  --umbriel umbriel --artifacts /tmp/umbriel-workspace-test
```

This exercises two outputs/windows, scrolling/dwindle/master layouts, floating,
scratchpads and virtual-output lifecycle. Targeted baseline: Umbriel 0.1.0 at
`297a787801e3b933cfa871521a4b586a98caef7f` (see [IPC docs](https://docs.noctalia.dev/umbriel/ipc/)).
