# Focus metrics fix and local compatibility deployment

Date: 2026-10-22. Goal: `d2bafa393f804790832e308df64934d5`.

## Collector changes

- Join workspace bindings' `project-id` to projd's live catalog for project names.
  Old `name`/`path` relation metadata is not authoritative.
- Keep workspace/project context subscribed independently of window/app/output
  enrichment; optional reads cannot hold the previous workspace's timers.
- Prefer manual workspace names, otherwise current project name or `empty`.
- Publish observed focus intervals before awaiting goal/workday persistence.
- Count known switches using window/project IDs when names are still pending.
- Preserve event-driven interval accounting and all historical measurements.

The current source suite passes 77 tests, with two explicitly opt-in tests
ignored. A private-bus regression blocks app metadata while changing focus and
checks complete 300-second intervals for both project and workspace. A negative
control without the optional-enrichment fix reproduced the stall.

## Local deployment compatibility

The running workspace producer is still the older deployment (`:1.5982`,
PID 979958) and does not implement `Output1.Enabled`. The separately pending
monitor/session rollout requires deploying the producer and newer session code
together; tracked goal: `97f76e91bb4f4dbe97fce48eeeefc565`.

A normal build from the current tree exposed this mismatch in session capture
during live verification. The final local deployment therefore retains the
compatible session implementation from `60eb95c` and incorporates only the
current metrics/focus modules. This avoids restarting the workspace producer or
performing the source-owner/association handoff as part of a metrics fix.

Build snapshot: `/tmp/opencode/steward-metrics-compatible`.
It is a `git archive 60eb95c` extraction, not a Git worktree or registered project.
Its `steward/src/main.rs` overrides only two modules:

```rust
#[path = "/absolute/current/checkout/steward/src/focus.rs"]
mod focus;
#[path = "/absolute/current/checkout/steward/src/metrics.rs"]
mod metrics;
```

The metrics module uses relative explicit child-module paths so the same source
is compiled in both contexts. All other components and dependencies in this
local compatibility build come from the archived baseline. The compatibility
suite passes 72 tests (two opt-in tests ignored), including the same metrics
regressions. Its release artifact was atomically installed as
`~/.local/bin/rsynapse-steward` and restarted at 17:12:14 PDT.

Installed SHA-256:
`1f0b1f385acb89e22fe1ba0d8f4189ee909008595c8e12875c804a7a06b9cd19`.

Reproduction uses a neutral working directory and the existing isolated public
Cargo cache/target, with two jobs and incremental compilation disabled:

```sh
env CARGO_HOME=/absolute/current/checkout/steward/.cargo-home \
  CARGO_TARGET_DIR=/absolute/current/checkout/misc/steward/target \
  CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
  cargo build --offline --locked --release \
  --manifest-path /tmp/opencode/steward-metrics-compatible/steward/Cargo.toml
```

After the coordinated producer/session upgrade, use the ordinary current-tree
build again. Until then, an ordinary current-tree deployment reactivates the
newer session implementation which requires `Enabled`.

## Live verification

Fresh project durations and presence gauges now reach Graphite; before the fix,
the recent project series had no measurements. Workspace names advance between
`coro-conf`, `office`, and `empty` during normal user activity. The collector's
read-only probe agrees with uncached public workspace identity reads and resolves
the current project name from projd. The compatibility deployment is active with
zero automatic restarts and no `Enabled` errors. The producer owner/PID is unchanged.

Graphite's existing focus files already use `sum` and `xFilesFactor=0`; no storage,
retention/dashboard edits, historical rewrites, manual project binding changes,
or window moves were performed.
Earlier missing intervals were not reconstructed or reassigned speculatively.
