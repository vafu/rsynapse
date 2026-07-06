# gtk4-background-effect Review

Status: reviewed

## Scope

Reusable GTK4 background/effect crate under `shell/core`.

## Review Order

1. `shell/core/background-effect/Cargo.toml`
2. `shell/core/background-effect/src/lib.rs`
3. `shell/core/background-effect/src/region.rs`
4. `shell/core/background-effect/src/effect.rs`
5. `shell/core/background-effect/src/test.rs`

## Crate Map

- `src/lib.rs` exposes `BackgroundEffect`, `BackgroundEffectRegion`, and
  `apply_background_effect`.
- `src/region.rs` builds rectilinear region rectangles, including rounded and
  inset rounded approximations.
- `src/effect.rs` bridges GTK/GDK Wayland handles into `wayland-client`, binds
  `ext-background-effect-v1`, installs blur regions, refreshes regions from GTK
  layout changes, and owns handle cleanup.
- `src/test.rs` covers pure geometry behavior.

## Findings

- Low: `apply_background_effect` is effectively one-shot for a window. If a
  handle already exists, `install_background_blur` returns without updating the
  requested region, so a later call with a different `BackgroundEffectRegion`
  is ignored. Relevant code:
  `shell/core/background-effect/src/effect.rs:82`.
- Low: repeated calls to `apply_background_effect(..., Blur(_))` can install
  additional `map`/`unrealize` signal handlers before the existing handle check
  runs. This is unlikely for static shell window config but worth documenting or
  guarding if the API becomes dynamic. Relevant code:
  `shell/core/background-effect/src/effect.rs:60`.
- Test gap: tests cover region geometry, but not GTK/Wayland lifecycle behavior:
  map/unrealize cleanup, dynamic child-tree refresh, repeated apply calls, or
  compositor-not-present no-op behavior.

## Refactor Ideas

- Document `apply_background_effect` as a static setup call, or support updating
  an existing `BackgroundEffectHandle` when the requested region changes.
- If dynamic calls are expected, store signal-handler IDs with the window data
  and ensure map/unrealize hooks are installed only once.
- Add a small seam around Wayland binding/handle creation so lifecycle behavior
  can be tested without a real compositor.

## Open Questions

- Is background effect configuration intended to change at runtime, or is it
  only applied from static `WindowConfig` during window creation?

## Verification

- `cargo test -p gtk4-background-effect --manifest-path shell/Cargo.toml`
  passed: 5 tests.
