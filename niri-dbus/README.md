# niri-dbus

`niri-dbus` will expose niri IPC state over the session D-Bus.

The intended role is narrow:

- Connect to niri through `NIRI_SOCKET`.
- Mirror outputs, workspaces, windows, focus, and relevant events as D-Bus
  objects, properties, and signals.
- Avoid shell-specific UI policy.
- Let consumers such as `rsynapse-shell` subscribe with `zbus` instead of
  reading from FUSE.

Current implementation status:

- owns `org.rsynapse.Niri` on the session bus;
- exports root, output, workspace, and window interfaces;
- registers dynamic objects under an ObjectManager root;
- folds live workspace/window/focus state through `niri_ipc::state::EventStreamState`;
- refreshes output details at startup/reconnect;
- removes dynamic objects on disconnect/reconnect.

V1 remains read-only. Command methods are intentionally out of scope.

Run:

```sh
cargo test
cargo run
busctl --user tree org.rsynapse.Niri
```
