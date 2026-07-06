# shell-examples-window-tiles Review

Status: reviewed

## Scope

Example shell widget/application for window tiles.

## Findings

- **Medium - the example duplicates the niri D-Bus model instead of importing a
  shared generated client.** `shell/examples/window-tiles/src/niri.rs:3` defines
  `org.rsynapse.Niri1` and `org.rsynapse.Niri1.Window` locally. That keeps the
  example self-contained, but it also repeats the app/niri-dbus boundary drift
  seen elsewhere in the repo.
- **Low - the top-level list model subscribes through generated macro code, but
  there is no test that the generated model compiles into a working component
  subscription.** `shell/examples/window-tiles/src/window.rs:26` is the useful
  example path for `#[view_model]`; the current tests only prove type names and
  local model formatting.

## Refactor Ideas

- Introduce a shared `niri-dbus-client` or protocol crate and have the example
  import its generated model types.
- Add a compile-time macro fixture, preferably through `trybuild`, that covers
  the `#[dbus_model]` plus `#[view_model]` path used here.

## Open Questions

- Should window tile examples remain in `shell/examples`, or should the current
  shell app own a small reusable window-list widget once niri-dbus stabilizes?

## Verification

- `cargo test -p rsynapse-window-tiles-example --manifest-path shell/Cargo.toml`
  passed; 3 tests.
