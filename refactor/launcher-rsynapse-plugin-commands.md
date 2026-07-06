# launcher-rsynapse-plugin-commands Review

Status: reviewed

## Scope

Command launcher plugin implementation.

## Findings

- **High - configured commands run synchronously during search.** On every
  matching query, `query` calls `execute_command` at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:240`; that runs
  `sh -c` and waits for all output at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:115`. A slow, hanging, or
  noisy configured command blocks the launcher daemon search path.
- **High - regex captures are interpolated into shell commands without
  escaping.** Capture strings are substituted with plain `replace` at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:107` before execution via
  `sh -c` at `shell/launcher/rsynapse-plugin-commands/src/lib.rs:115`.
- **Medium - default result IDs collide across command outputs.** JSON results
  without ids become `commands-{index}` at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:169`; each command output
  starts enumeration from zero, and `query` extends a shared results vector at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:240`. Colliding ids make
  daemon cache lookup ambiguous.
- **Medium - command output is unbounded.** `Command::output` at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:115` buffers stdout and
  stderr completely before parsing.
- **Low - invalid regex configuration is silently ignored.** `Regex::new` errors
  are dropped by `filter_map` at
  `shell/launcher/rsynapse-plugin-commands/src/lib.rs:75`.

## Refactor Ideas

- Add timeout, output-size limits, and cancellation to command execution.
- Replace shell-string templates with structured command argv templates, or make
  shell evaluation an explicitly unsafe/opt-in command mode.
- Include command identity in generated fallback result ids.
- Add parser and execution tests with a fake command runner.

## Open Questions

- Is the command plugin intended as a trusted personal automation feature only,
  or should it have guardrails suitable for shared configuration?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  plugin-commands tests, with the shared FFI-safety warning.
