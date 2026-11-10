# Widget And Source Instructions

Widgets own their view models and display policy. Reusable D-Bus -> Observable
adapters live in `shell-source` and are consumed directly.

## Lists Reconcile, Items Subscribe (mandatory)

Any collection of widgets — bar lists, popover contents, dropdowns — has two
strictly separate update paths:

1. **Structure** (add/remove/move/filter) is the only thing that may reconcile
   rows. Drive it from a structural list source of stable keys via
   `#[bind_list(...)]` or `set_component_list*`.
2. **Data** (status, title, icon, counts, connected state, …) is owned by each
   row through its own `#[source(per_key_source(key))]`.

Never derive row existence from a full data snapshot, and never insert/append/
remove child widgets imperatively in `update`/`init` to mirror snapshot data.
"The list is small" or "it is in a popover" is not an exception. See
`shell/CORE_WIDGETS_GUIDE.md` ("Lists Reconcile, Items Subscribe").

- Keep each widget module self-contained. `mod.rs` declares the Relm4/GTK
  component and the public-facing model/init types for that widget module.
- Widget-local source files compose `shell-source` adapters into view models.
  Do not add a top-level `src/sources` module.
- Declare reusable D-Bus descriptors, signal watchers, or service adapters in
  `shell-source`, not in a widget crate; widget files keep only display policy.
- A source provider declares one view model shape near the top of the file and
  exposes provider functions returning `Observable<ViewModel>` or
  `Observable<Option<ViewModel>>` for component `#[source(...)]` bindings.
- Keep files under 300 lines. If a widget has multiple meaningful states or
  submodels, create a subdirectory; its `mod.rs` declares only public-facing
  types/functions and composes private sibling files.
- Prefer enum-shaped view model parts when Relm4 view code can match them
  cleanly. If enum unpacking makes the view awkward, use subcomponents per
  submodel instead.
- Keep helper structs, source composition, D-Bus descriptor construction,
  decoding policy, and formatting helpers private in the same widget module
  unless another widget actually needs them. Generic adapters another crate
  needs belong in `shell-source`.
- Prefer composing `shell_core::source` observables with Rx operators directly.
  Do not introduce local mini source APIs around property, signal, or object
  manager sources; source-level behavior belongs in `shell_core::source`.
- Consumer sources must depend on shell-source observable helpers or typed
  service clients. Do not add filesystem watch clients to `rsynapse-shell`;
  request or add a shell-source observable primitive instead.
- Prefer Rx-native operators and shell-exposed helpers/types over custom
  implementations. If a source needs a truly custom observable, parser,
  watcher, cache, or adapter, document why existing Rx operators or
  `shell_core::source` primitives cannot express it cleanly.
- Do not hardcode widget heights in widget CSS or GTK builders. Only the bar
  height itself may define vertical size; child controls should use natural
  sizing, alignment, and padding instead of `min-height`, `height-request`, or
  fixed-height setters.
