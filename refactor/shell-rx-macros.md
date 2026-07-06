# shell-rx-macros Review

Status: reviewed

## Scope

Reactive macro support crate for shell framework/application code.

## Review Order

1. `shell/core/rx-macros/Cargo.toml`
2. `shell/core/rx-macros/src/lib.rs`

## Crate Map

- `src/lib.rs` exports the declarative `combine_latest!` macro and tests its
  tuple flattening/mapping behavior.

## Findings

- No correctness issues found in this pass.
- Low: `combine_latest!` has explicit arms up to nine sources. Calls with more
  sources fail at macro matching time rather than with a tailored diagnostic.
  Relevant code: `shell/core/rx-macros/src/lib.rs:25`.

## Refactor Ideas

- Document the nine-source cap in the macro docs, or add an explicit fallback
  arm with a clearer compile error for too many sources.

## Open Questions

- Should new code prefer `shell-core::source::combine_latest`/`Source` helpers
  over this RxRust-only macro during the `Observable` to `Source<T>` migration?

## Verification

- Initial mistaken command `cargo test -p rx-macros --manifest-path
  shell/Cargo.toml` failed because the package is named `shell-rx-macros`.
- `cargo test -p shell-rx-macros --manifest-path shell/Cargo.toml` passed: 3
  tests and 1 doctest.
