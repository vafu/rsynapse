# niri-dbus

`niri-dbus` exposes niri IPC state over the session D-Bus.

The role is narrow:

- Connect to niri through `NIRI_SOCKET`.
- Mirror outputs, workspaces, windows, focus, and relevant events as D-Bus
  objects, properties, and signals.
- Avoid shell-specific UI policy.
- Let consumers such as `rsynapse-shell` subscribe with `zbus` instead of
  reading from filesystem-backed shell state.

## Current Surface

- Owns `org.rsynapse.Niri` on the session bus.
- Exports root, output, workspace, and window interfaces.
- Registers dynamic objects under an ObjectManager root.
- Folds live workspace/window/focus state through
  `niri_ipc::state::EventStreamState`.
- Refreshes output details at startup/reconnect.
- Removes dynamic objects on disconnect/reconnect.

V1 is read-only. Command methods are intentionally out of scope.

D-Bus object paths are live object locations. Durable Locus relations should
prefer typed stable keys such as `org.rsynapse.niri.output.name`,
`org.rsynapse.niri.workspace.id`, `org.rsynapse.niri.workspace.name`, or the
live-window-scoped `org.rsynapse.niri.window.id`.

## Commands

From this directory:

```sh
cargo test
cargo run
busctl --user tree org.rsynapse.Niri
```

From the repository root:

```sh
cargo test --manifest-path niri-dbus/Cargo.toml
```
