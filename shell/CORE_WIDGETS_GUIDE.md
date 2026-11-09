# Core Widgets Guide

This guide describes the idiomatic way to build shell widgets in this
repository. It is an architecture guide, not a style essay: code that does not
follow these rules should be treated as unfinished migration work.

Use this guide with `SOURCE_API.md`, `shell/AGENTS.md`, and
`app/src/widgets/AGENTS.md`.

## Core Rule: Relm4 Everywhere

Every interactive widget is a Relm4 component. Views are declared with
`relm4::view!`; state is plain model data; messages are explicit `Input`
enums; child rows are Relm4 components.

Do not hand-build large GTK widget trees in `init` when a `view!` can express
the same tree. Imperative GTK is appropriate only for things the view macro
cannot express cleanly, such as controllers, gestures, popovers, or dynamic
widgets owned by a child component.

## Core Rule: Lists Reconcile, Items Subscribe

Lists must obey two separate update paths:

1. **Container changes**: add, remove, move, filter, or reorder list items.
   Only these changes may reconcile/rebuild the row collection.
2. **Item changes**: title, status, icon, count, progress, active state, and
   other row data. These must be handled by the item component itself, usually
   with `#[source(...)]` and a local view binding.

Never rebuild a list because a property of one row changed.

Bad pattern:

```rust
// Replaces every row object whenever any task status changes.
let rows = build_all_rows(&catalog);
store.splice(0, store.n_items(), &rows);
```

Good pattern:

```rust
// Parent reconciles only structural list descriptors.
#[bind_list(rows, row = TaskRow)]
rows -> gtk::Box { ... }
```

```rust
// Each row subscribes to its own live data.
#[shell_macros::model(module = task_row_sources)]
struct TaskRow {
    task: TaskRowInit,

    #[source(goal_status(task.key.clone()))]
    status: String,

    #[source(goal_tracked(task.instance_id.clone()))]
    active: bool,

    #[source(task_note_count(task.notebook.observable(), task.instance_id.clone()))]
    note_count: u32,
}
```

`TaskRowInit` must identify the row and contain structural/display policy
needed to create it. Do not put high-frequency mutable properties in
`TaskRowInit` unless the parent genuinely needs them to decide list structure.

If a mutable display field must change without changing list structure, it
belongs in the row’s source model, not in the parent list descriptor.

## Shell-Core Macros

### `#[shell_macros::model]`

Declares a Relm4 model whose selected fields are bound from observables.

```rust
#[shell_macros::model]
struct Widget {
    plain_state: bool,

    #[source(source::thing(plain_state))]
    bound_state: ThingView,
}
```

Generated behavior:

- creates the source sidecar module (`sources` by default);
- generates `sources::Msg`, `sources::Field`, dirty tracking, errors;
- starts subscriptions from `init`;
- updates model fields when observables emit;
- clears changed flags after `post_view`.

Model fields are plain values. Do not store observables in model fields unless
they are explicitly part of a source expression or a child-init descriptor.

### `#[shell_macros::component]`

Connects a Relm4 component to the model’s generated source messages.

```rust
#[shell_macros::component(model = Widget)]
#[relm4::component(pub(crate))]
impl SimpleComponent for Widget {
    type Init = WidgetInit;
    type Input = sources::Msg;
    type Output = ();

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        Widget::update(self, msg);
    }
}
```

Use `model = Type` when the component reads bindings from a
`#[shell_macros::model]` struct. Use explicit binding syntax only for older
single-field components that do not have a model.

### `#[shell_macros::bindings]`

Use for focused, binding-only components where a full model struct would be
noise. It maps generated source messages to a component `Input` variant.

### `#[bind_list(field, row = RowComponent)]`

Declares a reactive child-list binding in `view!`.

```rust
#[bind_list(items, row = ItemRow)]
items -> gtk::Box {
    set_orientation: gtk::Orientation::Vertical,
}
```

The widget type selects the backend. Today the implemented backend is
`ComponentListBoxExt` for `gtk::Box`. Rows are reconciled by item equality.
If the row emits `Output`, use the explicit shell-core forward API described
below instead of `#[bind_list]`.

### `#[shell_macros::view_model]`

Use for components where a single source model drives the whole component and a
separate model struct is unnecessary. It injects subscriptions into `init` and
generates the source sidecar.

### `#[shell_macros::dbus_model]`

Use for generated/declarative D-Bus model bindings. Keep generated/protocol
shape in the appropriate schema/codegen path; do not hand-patch generated code.

## Shell-Source Primitives

Widgets compose typed observables from `shell_core::source`.

### `Observable<T>`

Use `Observable<T>` throughout. Model fields store `T`, not `Observable<T>`.

### `StateSignal<T>`

Use for local reactive state that multiple widgets or sources must observe.

```rust
let selected = StateSignal::new(None);
let status = source::status(selected.observable());
```

`StateSignal` replays its current value and emits only distinct changes. It is
the correct way to let child rows subscribe to parent-local selection or mode.

### `combine_latest!`

Use `shell_rx_macros::combine_latest!` for fixed-arity heterogeneous source
composition.

```rust
let vm = combine_latest!(
    title(),
    status(),
    active() => |(title, status, active)| Vm { title, status, active }
);
```

### `switch_map`

Use `source::switch_map` when the inner source changes with selected
project/workspace/task/notebook.

### `shared_by_key`

Use `source::shared_by_key` for sources many rows can request. Shared sources
must replay the latest value, start on first subscriber, and stop on last
subscriber drop.

Do not create one D-Bus/file/service watcher per row when every row can share
the same upstream observable.

### `diff`, `diff_by_key`, `diff_slices`, `diff_slices_by_key`

Use these for incremental collection updates. Prefer key-based diffs when
items have stable IDs.

## Shell-Core List Reconciliation

`shell_core::list::ComponentListBoxExt` reconciles child Relm4 components into
either a `gtk::Box` or a `gtk::ListBox`.

Use:

- `set_component_list(...)` when item equality is a valid identity and row
  outputs are not needed.
- `set_component_list_by_key(..., key)` when item data may change in place but
  row identity is stable.
- `set_component_list_forward(...)` when row components emit outputs to a
  parent.
- `set_component_list_by_key_forward(...)` for keyed rows with outputs.

Use `GtkListBox` when rows should have native Adwaita list semantics. Add the
`navigation-sidebar` CSS class to get flat rounded rows and native hover. A row
root may itself be a `GtkListBoxRow`/`AdwActionRow`; in that case `GtkListBox`
uses it directly rather than generating a wrapper, so selection identity should
read the row's own widget name first and fall back to its child.

Selection belongs to the `GtkListBox` for list surfaces. Keep a parallel
`StateSignal<Option<Key>>` only when other widgets need the selection; sync it
from `row-selected` and call `select_row`/`unselect_all` after structural
reconciliation so selection survives list updates.

Rows are reused across moves and unchanged updates. A row may be recreated when
its structural init changes, but it must not be recreated merely because its
own live properties changed.

`ComponentListUpdate::<Row>::new(&items)` is the standard update value.

## Parent/Child Data Flow

Use this split:

- Parent owns selection, filters, collapsed sets, and list structure.
- Row owns live task/note/workspace/window properties and local row actions.
- Shared state that both need (selection, mode, notebook) is passed as a
  `StateSignal<T>` in row init and observed by the row.
- Row actions flow back as component `Output` and are forwarded to the parent
  input sender by shell-core list forwarding.

Do not pass a parent `ComponentSender` through row init unless the row is a
one-off child not managed by a list. For list rows, output forwarding keeps
the row reusable and testable.

## Source Ownership

- Reusable service adapters live in `shell-source`.
- Widget-specific display policy and view models live beside the widget.
- Do not add a top-level widget `sources` module.
- Prefer semantic source functions (`goal_status`, `goal_tracked`,
  `task_note_count`) over raw D-Bus descriptors in views.
- Avoid polling, sleeps, debounce-based correctness, and filesystem watchers.

## Custom Input Components

When a component needs source messages plus local actions, define a local input
enum and implement `From<...sources::Msg>`:

```rust
enum Input {
    Source(sources::Msg),
    Activate,
}

impl From<sources::Msg> for Input {
    fn from(msg: sources::Msg) -> Self {
        Self::Source(msg)
    }
}
```

In `update`, delegate generated messages first, then handle local actions:

```rust
match message {
    Input::Source(msg) => Self::update(self, msg),
    Input::Activate => ...,
}
```

## View Binding Rules

- Use `#[watch]` for fields that may emit repeatedly.
- Use `#[track(...)]` for list bindings and coarse structural rebuilds.
- Keep row view setters local to row-owned data.
- `post_view` may clear changed flags once auto-injected by the component
  macro; do not mix manual and generated list rebuilds for the same field.
- Do not call `remove_all`, `splice(0, n_items, ...)`, or equivalent full-list
  rebuilds in response to an item property change.

## Testing Checklist

For any list migration:

1. Unit-test structural view-model changes.
2. Verify unchanged item property updates do not change list identity.
3. Verify add/remove/move still reorders correctly.
4. Run the widget tests.
5. Run `cargo check`/`cargo test` for the crate.
6. Smoke-test the live surface if it is runtime UI.

## Migration Checklist

- [ ] Widget is a Relm4 component with `view!`.
- [ ] State is in model fields; sources use `#[source]`.
- [ ] Child lists use shell-core reconciliation.
- [ ] Rows subscribe to their own live properties.
- [ ] Parent rebuilds only on structural list changes.
- [ ] Shared upstream sources use `shared_by_key`.
- [ ] Row outputs are forwarded instead of embedding parent senders.
- [ ] No full-list rebuild on item property changes.

## Findings From The Tasks Migration

The Remarks task list migration confirmed several details worth making
explicit:

- `StateSignal<T>` is passed in row init, but it must implement `Debug` and
  `PartialEq` because row init participates in list reconciliation and Relm4
  init debugging. `shell-source` provides pointer-based implementations for
  this.
- List forwarding must target the parent's **input** sender, not
  `output_sender()`. A parent with `Output = ()` will otherwise compile-fail at
  the forwarding call.
- `ComponentSender::output()` is how a row sends `TaskRowOutput` to its parent.
  The shell-core forwarding list then maps that output into a parent input.
- Row init is also a structural identity. Keep only fields that define list
  structure or stable child dependencies in it. Put live values such as task
  status, title, active state, and note count in `#[source]` model fields on
  the row.
- Use `set_component_list_by_key_forward` for keyed parent lists whose rows
  emit actions. The task list uses immutable task instance IDs as keys.
- A row may carry shared `StateSignal`s for parent-local state (selection and
  notebook/tag counts) while still subscribing to live row data itself.
- `TagCounts` needed `PartialEq` so it can live in a `StateSignal` carried by
  row init. Domain values used in signals should be comparable and stable.
- Keep one live signal instance for the whole list when rows share it. Do not
  construct a fresh `StateSignal` on every refresh unless you intend to restart
  every row subscription.
- Parent list refreshes should only be triggered by structural changes
  (catalog membership, filtering, collapse state), not by a row property
  update. Row property updates and resulting view changes stay inside the row.
- For `GtkListBox`, `AdwActionRow` is already a `ListBoxRow`. Do not require a
  child widget for selection identity; check the row itself.
- `ListBox` selection is persistent only while selected rows survive
  reconciliation. Use keyed reconciliation and re-apply selection after
  structural list updates.
- Keyed reconciliation reuses a row only when both its key and its structural
  init value are unchanged. If the key matches but the init differs, only that
  row is recreated. Keep live properties out of init so normal updates do not
  take this path.

## Findings From The Widget Migration Pass

- The bzbus D-Bus watcher belonged in `shell-source`; widget code now keeps only
  presentation mapping and per-row view policy.
- GitHub and notification rows both needed structural init plus per-item live
  sources. Keying alone was not enough: the old keyed diff retained stale init
  data until the reconciler was changed to recreate only changed-key init.
- `GtkListBox` may use a component root directly when that root is already a
  `ListBoxRow` (`AdwActionRow`). Row identity helpers must check the row itself
  before checking its child.
- Shared row state should be one long-lived signal per list. Workspace selector
  and notifications both pass stable `StateSignal`s into row init rather than
  rebuilding signals during list refreshes.
- Components that own actions should emit `Output`; parent list reconciliation
  forwards those outputs into parent inputs. This keeps row components reusable
  and avoids passing parent senders through list init.
