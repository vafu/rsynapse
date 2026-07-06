# shell-examples-volume-status Review

Status: reviewed

## Scope

Example shell widget/application for volume status.

## Findings

- **Medium - unsubscribe can miss the child process and then block joining.**
  `subscribe` spawns a thread at
  `shell/examples/volume-status/src/main.rs:247`, but the `pactl subscribe`
  child is only stored after spawn/stdout setup at
  `shell/examples/volume-status/src/main.rs:274`. If the subscription is
  dropped before that assignment, `stop` observes no child at
  `shell/examples/volume-status/src/main.rs:336` and then joins the thread at
  `shell/examples/volume-status/src/main.rs:346`. The thread can then create the
  child and block forever reading `pactl subscribe`, leaving unsubscribe hung.
- **Low - the example teaches a custom Rx observable for process streams.** It
  is valid example code, but it is easy to copy into production shell surfaces.
  New examples should prefer the current `shell_core::source` task/process
  helpers, or introduce one if the helper does not exist yet.

## Refactor Ideas

- Move process-backed source construction into a reusable shell-core helper with
  explicit cancellation semantics, then make this example a thin parser/UI
  demonstration.
- Add a lifecycle test around immediate unsubscribe if the process source is
  kept local to the example.

## Open Questions

- Should audio status eventually come from a typed session service instead of
  shell widgets spawning `pactl` directly?

## Verification

- `cargo test -p rsynapse-volume-status-example --manifest-path shell/Cargo.toml`
  passed; 2 tests.
