# Workspace D-Bus contract, version 1

This is the implementation target for both Niri and Umbriel, owned by the
independent `workspace-dbus-api` crate. The Rust proxy declarations in
`api/src/proxies.rs` define the property types. Names and paths live in
`api/src/lib.rs`; no consumer needs a compositor protocol dependency.

## Service and ownership

| Item | Value |
| --- | --- |
| Bus | Session |
| Service | `org.rsynapse.Workspace` |
| Manager / ObjectManager root | `/org/rsynapse/Workspace` |
| Manager interface | `org.rsynapse.Workspace.Manager1` |
| Output interface | `org.rsynapse.Workspace.Output1` |
| Workspace interface | `org.rsynapse.Workspace.Workspace1` |
| Window interface | `org.rsynapse.Workspace.Window1` |

Export `org.freedesktop.DBus.ObjectManager` at the root and standard Properties
on each object. Collections contain live object paths, not serialized protocol
payloads. New objects appear through `InterfacesAdded`; removed objects through
`InterfacesRemoved`. Mutable properties emit `PropertiesChanged`.

The service owns compositor observation, normalization and optional compositor
controls. It must not
depend on projd, steward or Locus or read/write their state. Projects, goals,
workspace naming overrides, associations and work attribution are outside this
contract. Steward/Locus react to object lifecycle and apply their own relation
policy. Steward's workspace lifecycle reconciler performs association cleanup;
the projection has no Locus dependency.

## IDs, references and lifecycle

- Workspace and window `Id` are nonempty opaque D-Bus strings (`s`). Niri IDs
  can be converted to strings; Umbriel IDs do not need numeric aliases. Clients
  must not parse numeric values, compositor prefixes, connector names, or
  indices out of them. `"7"` and `"007"` are distinct IDs.
- IDs are immutable for an object's lifetime and distinct within its collection
  in the current projection. A workspace's display index and name may change
  without changing its identity. Output `Name` identifies its connector.
- A removed workspace ID is not reused within one source generation. Umbriel
  can restart native workspace counters when a monitor reconnects; the backend
  assigns a fresh public ID to that new lifetime and rewrites all references.
  Public IDs must not be joined directly to native compositor IDs by consumers.
- IDs are live identities scoped by the well-known service's unique bus owner
  and `Generation`, not persistent cross-session keys. New bus owner or
  generation means consumers must reconcile their live bindings. Steward/Locus
  decide which relations expire and which explicit persistent preferences can
  be reapplied. A backend switch must reset the projection; matching raw IDs
  alone never authorizes transferring an association.
- References use D-Bus object paths (`o`). Optional references use arrays of
  **zero or one** object paths (`ao`), matching existing source helpers. There
  is no `0`, empty string, `/`, or fake object as an absence sentinel.
- Optional scalar values also use zero-or-one arrays. An empty value array
  means unknown/not applicable, not a fabricated successful observation.
- The path helpers encode arbitrary ID bytes into valid, injective segments.
  They are implementation conveniences. Clients follow references and read
  `Id`/`Name`; they must not recover identity from path text.
- `Connected` becomes true after the initial inventories are ready. On source
  loss, clear focus and collections, remove dynamic objects and emit the
  corresponding lifecycle/property changes. `Generation` advances on resets
  and a new connected projection, not each ordinary state update. Bus-owner
  loss is also loss of observation; a restarted process may restart its counter.
- References must resolve to registered objects. Register added targets before
  publishing references. Remove references to departing targets and unregister
  those objects as part of the same logical update. D-Bus does not provide an
  atomic multi-object snapshot: consumers subscribe before their initial read
  and reconcile subsequent signals. Do not use a sleep/debounce as a consistency
  mechanism.

## Manager properties

All are read-only and emit changes.

| Property | Signature | Meaning |
| --- | --- | --- |
| `Connected` | `b` | Initial live inventory is ready |
| `Generation` | `t` | Projection reset epoch within the current bus owner |
| `Outputs` | `ao` | All connected output objects |
| `Workspaces` | `ao` | All live workspace objects |
| `Windows` | `ao` | All live mapped window objects |
| `FocusedOutput` | `ao` | Zero or one globally selected output |
| `FocusedWorkspace` | `ao` | Zero or one globally selected workspace |
| `FocusedWindow` | `ao` | Zero or one application with global keyboard focus |

## Output properties

| Property | Signature | Meaning |
| --- | --- | --- |
| `Name` | `s` | Connector name; immutable for this object |
| `Focused` | `b` | This is `Manager.FocusedOutput` |
| `CurrentWorkspace` | `ao` | Zero or one active workspace on this output |
| `Workspaces` | `ao` | Workspace membership, ordered by display index |

## Workspace properties

| Property | Signature | Meaning |
| --- | --- | --- |
| `Id` | `s` | Opaque immutable identity |
| `Name` | `as` | Zero or one explicit compositor name; anonymous means empty array |
| `Layout` | `s` | Effective tiling mode: `scrolling`, `master`, `dwindle`, or `unknown` |
| `Index` | `u` | One-based display position on its output, not identity |
| `Output` | `ao` | Zero or one containing output |
| `Active` | `b` | Current on its output, independently of global focus |
| `Focused` | `b` | This is `Manager.FocusedWorkspace` |
| `Urgent` | `b` | Compositor reports workspace urgency |
| `ActiveWindow` | `ao` | Zero or one remembered focused member, even when the workspace is inactive |
| `Windows` | `ao` | Member windows; ordering alone is not a portable layout coordinate |

`Layout` reports the workspace's effective current mode, including runtime
overrides and per-workspace configuration. It emits `PropertiesChanged` on mode
changes even when focus and window membership are unchanged. Niri always reports
`scrolling`; Umbriel projects its workspace snapshot's `layout`. Unknown or absent
native modes report `unknown`, never an assumed scrolling layout. The API's Rust
`Layout` enum uses the same string wire representation and tolerates future modes
as `Unknown`. Initial ObjectManager snapshots include the effective mode.

## Window properties

| Property | Signature | Meaning |
| --- | --- | --- |
| `Id` | `s` | Opaque immutable identity |
| `Title` | `as` | Zero or one current title; changes are signalled |
| `AppId` | `as` | Zero or one application identity |
| `Pid` | `ai` | Zero or one positive process ID; unknown/invalid native PIDs are absent |
| `Workspace` | `ao` | Zero or one home workspace |
| `Output` | `ao` | Zero or one current output, independent of workspace membership |
| `Focused` | `b` | This is `Manager.FocusedWindow`, not remembered per-workspace focus |
| `Floating` | `b` | Outside the tiled layout |
| `Urgent` | `b` | Compositor reports window urgency |
| `ColumnIndex` | `at` | Zero or one zero-based scrolling column |
| `RowIndex` | `at` | Zero or one zero-based scrolling row |

Column/row are absent for floating windows and layouts without those coordinates.
They are optional presentation hints, never identity. A named workspace whose
literal name is `"2"` is not an anonymous workspace with display index 2.

## Normalized focus

At most one window, workspace and output are globally focused in each projection.
The manager references and object `Focused` flags describe the same selections.
Several workspaces may be `Active`, one per output; several may remember their
own `ActiveWindow` without those windows having global keyboard focus.

When an application window has global keyboard focus, select its current output
and home workspace when present. A workspace-less scratchpad window can still
have an output; do not fabricate a home workspace. When no application has
keyboard focus, `FocusedWindow` is empty. Use the keyboard-focused layer's
output when available, otherwise the compositor's selected output (including an
empty output), and that output's active workspace. On source loss, all focus
references are empty. A pointer move must not override an existing application's
keyboard-focus context.

The backend performs this mapping; the shell and steward do not know which
native field or socket protocol produced it. In particular, native remembered
workspace focus and pointer-preferred output flags cannot be copied blindly
into global application-focus fields.

## Scope and cutover

### Optional session controls

`org.rsynapse.Workspace.Control1` is an additive interface on the root. It owns
no saved sessions or project policy. `Capabilities: as` lists supported controls;
currently Umbriel advertises `select-workspace`, `set-layout`, `place-window`.
Niri continues projecting state and advertises an empty list.

Clients bind the control proxy to the source's **unique bus owner** and pass its
observed `Generation` (`t`) on each call. Disconnected/stale generations fail.
Completion waits for projected state changes; timeouts bound failure only.

- `SelectWorkspace(generation: t, output: s, index: u) -> id: s`: selects a
  one-based position on an existing output and returns the actual opaque live
  ID. A dynamic output may clamp to its trailing empty workspace. It does not
  promise persistent empty workspaces or create named configuration entries.
- `SetLayout(generation: t, workspace: s, layout: s)`: selects the identified
  workspace and sets a supported layout (`scrolling`, `master`, `dwindle`).
- `PlaceWindow(generation: t, window: s, workspace: s, floating: b)`: places a
  live window on the identified workspace and sets its floating state.

Commands are serialized. Umbriel actions may temporarily change focus. Native
selectors are resolved from current backend-owned projection properties, never
by parsing public IDs. A destination that expires during placement fails rather
than silently selecting a different workspace. Exact geometry/tiling restoration
and application launching are outside these methods.

This baseline covers current Rsynapse compositor-state consumption. It does not
require Niri tile geometry, focus timestamps, runtime config diagnostics,
compositor version, HDR/VRR inspection, or compositor-specific layout settings
beyond the effective workspace tiling mode.
Those can be added independently when a consumer needs them. Controls are the
optional extension above; the URL helper can be patched independently.

Implementation status:

Niri and Umbriel implementations serve this API. Shell/steward consumers and
activation/installation templates use the same contract, and association cleanup
belongs to steward. Backend selection and focus normalization stay in the
producer; no compositor-specific consumer or project/goal policy is required.

This is an intentional internal wire break: service/interface/path names change,
window/workspace `Id` changes from `t` to `s`, and workspace `Index` is normalized
from a protocol-sized byte to `u`. Ship the Niri producer and its consumers
together. Do not run a half-migrated consumer against the old service or infer
compatibility by casting an opaque ID back to a number. External agent/app/build
producers using window identity must adopt the same opaque identity contract.
