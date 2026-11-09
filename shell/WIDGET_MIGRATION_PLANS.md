# Widget Migration Plans

## Implementation Status (2026-11-09)

The first migration pass is implemented and verified across the shell workspace:

- bzbus watcher moved to `shell-source`; context-zone rows use stable build IDs
  and per-row subscriptions.
- GitHub pull-request rows use structural init plus a shared per-PR source;
  title/detail/unseen updates no longer recreate rows.
- Notification entries use per-notification `StateSignal`s; cards subscribe to
  live notification data and forward dismiss/action outputs.
- Workspace selector uses `view!`, keyed shell-core reconciliation, reactive
  entry rows, and key-based selection.
- Selected-project, goal, Bluetooth, and network clusters are extracted into
  Relm4 components; goal state is shared by output scope.
- Source errors use shell-core component reconciliation.
- Task rows use native `AdwActionRow`/`GtkListBox` selection and per-row live
  sources.

Verification: `cargo test --workspace` and `cargo check --workspace` pass for
the shell workspace. Remaining sections below are retained as design notes and
for follow-up hardening.

Generated from independent audits against `CORE_WIDGETS_GUIDE.md` on 2026-11-09.
Each section is a per-widget implementation plan; no widget audit edits were made
by the subagents.

## Bar

### context_zone — NEEDS MIGRATION (critical)
- `context_zone.rs` binds `Vec<ContextItem>` where `ContextItem::Build(BzBusView)`.
- `BzBusView` contains progress/tooltip/classes, so every progress tick replaces
  the row via equality reconciliation.
- Add stable build IDs: `bzbus::ongoing_build_ids() -> Observable<Vec<String>>`.
- Add `bzbus::build_view(id) -> Observable<Option<BzBusView>>` over the shared
  bzbus snapshot.
- Change list init to stable IDs. Make `ContextZoneItem` a
  `#[shell_macros::model]` row with `#[source(bzbus::build_view(id.clone()))]`.
- Bind icon/tooltip/progress/classes with `#[watch]`.
- Tests: progress-only update leaves IDs unchanged; add/remove changes IDs.

### source_errors — COMPLIANT (minor)
- Immutable historical rows; append/truncate is structural.
- Optional: key by `SourceError::id`, derive count from list length.

### workspace_bar — COMPLIANT
- `ProjectLabel` already subscribes to project/branch/git/active state.
- Optional: move `has_windows`/`named` into row subscriptions so placeholder-only
  changes don't reinitialize rows.

### window_tile — COMPLIANT (minor gaps)
- `WindowTile` subscribes to `window_tile_vm(window)`; only window membership
  rebuilds the list.
- Optional: use `set_component_list_by_key(..., path_key)` explicitly.
- If click-to-focus is added, emit row `Output` and forward through shell-core.

### systray — COMPLIANT
- Stable `TrayItemKey` list and per-row item/menu sources.
- Imperative popover menu rebuild is acceptable for native menu widgets.

### project_label — COMPLIANT (picker rebuild leak)
- Row source composition is correct.
- `sync_icon_picker` runs on every VM change. Gate it on a small picker-relevant
  key (project id, glyph, overridden state) rather than branch/tooltip changes.

### selected_project — NEEDS MIGRATION (component extraction)
- Source graph is good; view is inline in `MainBar`.
- Extract `SelectedProjectItem` component owning title/branch/git/spinner.
- Remove unused `focused_project` subscription.
- Convert `ProjectInitializer` mutations into component inputs/results.

### goals — NEEDS MIGRATION (component extraction)
- `goal_bar` source is good but not shared. Wrap with `shared_by_key`.
- Extract `GoalIndicator` component with click `Output::OpenRemarked`.
- Keep goal-slot measure overlay in the component or parent explicitly.

### bluetooth — NEEDS MIGRATION (inline widget)
- Source/model are compliant. Extract `BluetoothIndicator` component.
- Emit `Input::Toggle`; call toggle with currently bound adapter.

### network — NEEDS MIGRATION (inline widget)
- Source/model are compliant. Extract `NetworkIndicators` component.
- Let `view!` own ethernet/wifi boxes; remove `bar_item::container` local refs.

### bzbus — NEEDS MIGRATION (source ownership)
- Move raw D-Bus owner/object-manager/properties loop from widget module to
  `shell-source`.
- Expose `snapshots()`, `invocation(id)`, `ongoing_ids()`.
- Keep icon/tooltip/progress geometry display policy in the app crate.

### MainBar — NEEDS MIGRATION (structure)
- One ~640-line `view!` plus manual `bar_item::container` trees.
- Extract status clusters into components; remove local-ref-created widget trees.
- Move imperative click handlers into component inputs/outputs.
- Keep monitor reconciliation, output scope, request server, and overlay lifecycle
  in `MainBar`.

## GitHub Status

- **NEEDS MIGRATION.** `PrEntry` (title/detail/revision/unseen) is embedded in
  row init, so a PR update removes/reinserts the row.
- Add a shared per-PR source in `shell-source/github.rs`.
- Replace row init with structural `{url, category}`; bind live PR fields with
  `#[source]` and `#[watch]`.
- Keep category summary/count in `GitHubItem`; row actions stay local unless
  parent-visible errors are needed.
- Add tests: same URL + changed title keeps row identity; add/remove/move updates
  structure; `MarkSeen` uses the live revision.

## Notifications

- **NEEDS MIGRATION.** `NotificationCard::Init = NotificationView`, so any body,
  summary, urgency, image, or action change removes/reinserts the card.
- Introduce structural `NotificationEntry { id, generation, view: StateSignal<NotificationView> }`.
- Same-ID updates call `view.set(next)`; they do not replace row init.
- `NotificationCard` becomes a source-driven row around
  `NotificationRowInit { id, view }`.
- Reconcile action buttons from subscribed actions; emit `Dismiss` /
  `ActionInvoked` outputs and forward them.
- Move imperative card D-Bus client helpers into the app notification service.
- Convert session-lock polling into `#[source(session::locked())]`.

## Workspace Selector / MPRIS / OSD

### workspace_selector — NEEDS MIGRATION
- Hand-built content tree in `init`; move into `view!`.
- Rows should be `WorkspaceEntryRow` components, not manual `AdwActionRow`s.
- `Entry` mixes structural identity and live name/path/current state. Split into
  structural init + row sources.
- Replace `entries_signal`/manual `ListDiff` with shell-core keyed reconciliation.
- Use `StateSignal<Option<EntryKey>>` for selection, not row indices.
- Remove manual `clear_changed()` already injected by the component macro.

### mpris — COMPLIANT
- `#[shell_macros::model]`, `#[source(switch_map(...))]`, `StateSignal<bool>`,
  and custom input mapping are correct. No list surface.

### osd — COMPLIANT
- Proper async Relm4 component; no list surface.
- Push-based show/hide is an intentional event-driven overlay exception.

## Execution Order

1. shell-source bzbus extraction + stable IDs.
2. context_zone row migration.
3. GitHub per-PR source + row migration.
4. Notifications structural entries + row subscriptions.
5. Workspace selector component/list migration.
6. MainBar child-component extraction.
7. Remaining component extractions and keyed-list hardening.
