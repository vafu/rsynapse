# Rsynapse Shell Agent Guide

This repository is the Rsynapse shell monorepo. It contains reusable
Rust/Relm4 shell framework crates plus concrete Rsynapse UI surfaces.

The current architecture is DBus-first: shell code consumes typed DBus services
through `shell_core::source` observables. Do not add a FUSE/filesystem-backed
reactivity layer back into this workspace.

## First Steps

Before planning or editing, read:

- `PROJECT.md` for the project blueprint and constraints.
- `PLAN.md` for the current roadmap and crate boundaries.
- `SOURCE_API.md` when work touches source bindings, observable APIs, DBus
  source helpers, or macro ergonomics.
- `app/src/widgets/AGENTS.md` before changing concrete widgets or
  widget-local source providers.

Use `$locus-shell` and `rust-guide` for Rust, GTK/Relm4, source, and
framework-boundary work.

## Workspace Boundaries

- `core/shell-core` owns generic GTK/layer-shell app setup, window primitives,
  stylesheet loading, reusable Observable source primitives, and DBus source
  helpers.
- `core/background-effect` owns reusable GTK4 `ext-background-effect-v1`
  helpers.
- `core/macros` owns Relm4/model/source binding macros.
- `core/rx-macros` owns small RxRust composition macros only.
- `app` owns the current combined bar, OSD, notifications, request CLI/server,
  styling, widget-specific view models, and product policy.
- `launcher` owns the launcher workspace, including daemon, CLI, plugin, and UI
  crates until those packages are intentionally renamed or split.
- Future `bar`, `osd`, and `notifications` crates belong beside `app` when the
  combined app is intentionally split.
- Do not reintroduce removed `provider/*` crates, `ObservableSource<T>`, a
  custom provider task runtime, a filesystem watch transport, or a provider
  facade in this repo.
- DBus service implementation belongs in sibling service projects such as
  `../niri-dbus`, `../locus`, or `../../claude-dbus`. Shell code only consumes
  their public surfaces.

## Observable Source Contract

The source API is Observable-first.

- Widget model fields are plain values.
- Source expressions return `shell_core::source::Observable<T>`.
- Macro-generated glue subscribes to observables and updates Relm4 model state.
- Use Rx-native operators such as `map`, `filter_map`, `combine_latest`,
  `merge`, `switch_map`, `start_with`, and `distinct_until_changed`.
- Use `shell_rx_macros::combine_latest!` for fixed-arity heterogeneous source
  composition when plain RxRust chains are awkward.
- Keep handwritten async loops isolated inside small shell-core primitives that
  bridge external APIs into Observable form.

Sharing rules:

- Sources that many widgets/rows can request should use
  `source::shared_by_key(kind, key, || ...)`.
- Shared sources must replay the latest value to new subscribers, start
  upstream work on the first active subscriber, and stop upstream work when the
  last subscriber drops.
- Do not add local `OnceLock` caches or manual `.shared()` wrappers in widgets
  unless `shared_by_key` cannot express the descriptor.
- Do not use debounce, sleeps, or timeouts to hide source ordering bugs, list
  churn, or lifecycle problems. Time-based coalescing is only acceptable for
  inherently noisy external systems such as stylesheet reloads, and it must be
  named as such.

Consumer source rules:

- Consumers compose `shell_core::source` observables.
- Add a shell-core observable primitive when a backend capability is generally
  reusable.
- Keep concrete widget view models and display policy in app or surface crates.
- Prefer typed DBus descriptors and service helpers over raw string traversal at
  widget call sites.

## Widget Rules

- Keep each widget module self-contained. Source providers live beside the
  widget that consumes them.
- Do not add a top-level `src/sources` module.
- A provider should expose `Observable<ViewModel>` or
  `Observable<Option<ViewModel>>` for `#[source(...)]` bindings.
- Prefer enum-shaped view models when they simplify Relm4 view matching; use
  subcomponents when enum unpacking makes the view awkward.
- Keep helper structs, parsing, formatting, and path construction private unless
  another widget actually needs them.
- Do not hardcode widget heights in Rust or CSS. The bar height itself may set
  vertical size; child widgets should use padding, alignment, and natural size.
- Do not solve visual behavior by adding ad hoc graph traversal inside GTK
  component lifecycle methods.

## Request CLI/Server

- The Unix-socket request bridge is app product behavior, not a `shell-core`
  framework feature.
- Keep command names and policies such as `scheme-toggle` and `hints
  active|show|hide|toggle` in app unless another consumer needs the same
  transport contract.
- Direct `.config/ags` runtime usages should be migrated to app commands when
  the behavior now lives in Rust.

## Do

- Keep framework code generic and consumer policy in `app`, `launcher`, or
  future surface crates.
- Prefer existing `shell_core::source` primitives and Rx operators over custom
  source runtimes.
- Add focused tests for DBus descriptors, source filtering, parsing, and
  request-command behavior.
- Update `PLAN.md` or relevant refactor docs when architecture changes.
- Preserve nested AGENTS guidance; top-level rules are broad, nested rules win
  for their directories.

## Don't

- Do not add public one-shot read helpers or imperative clients to
  `shell-core::source`.
- Do not reintroduce schema-specific marker structs, generated-style path
  extension traits, `NodeRef`, `Property`, `Relation`, or a provider facade in
  this workspace.
- Do not hand-write generated schema APIs. If a graph concept needs generated
  helpers, change schema/codegen in the owning repository and run codegen.
- Do not patch generated files manually.
- Do not use timing hacks to make UI updates appear stable.
- Do not put UI cards inside cards or hardcode child widget heights.

## Verification

Useful commands:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo test --workspace
cargo fmt --check
```

Narrow checks:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo test -p shell-core source::support::tests
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo test -p rsynapse-shell request
```

Install/restart when the running shell should reflect changes:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo install --path app --locked --force --root /home/v47/.local
systemctl --user restart rsynapse-shell.service
systemctl --user status rsynapse-shell.service --no-pager
```

Live checks that often catch integration mistakes:

```sh
rsynapse-shell request hints toggle
rsynapse-shell request scheme-toggle
```
