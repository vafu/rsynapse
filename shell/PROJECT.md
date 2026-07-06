# Locus Shell Project Blueprint

## Objective

Locus Shell is a Rust/Relm4 framework for high-performance, low-footprint
desktop shell widgets such as bars, OSDs, and notifications. It replaces
heavier GJS/AGS-style shell code with native GTK4 binaries and typed reactive
source helpers.

The shell should provide a concise authoring model for widgets while preserving the runtime characteristics of compiled Rust and GTK4.

## Core Constraints

- No JavaScript engine or embedded interpreter. Reactive source composition, if
  used, must be compiled Rust owned by the shell framework.
- Consumer shells decide their process grouping. A production shell may split
  major surfaces into binaries, but `rsynapse-shell` currently runs the bar and
  OSD in one process with multiple layer-shell windows.
- Widget process isolation is a consumer policy choice, not a `shell-core`
  responsibility.
- Live UI state should use D-Bus-backed observable source helpers built on
  `zbus`. Filesystem-backed reactivity is not part of the active architecture.
- Sources such as UPower, time, weather, media, D-Bus services, and custom user logic
  enter the UI through typed source expressions. The public source API is
  Observable-first; the old `ObservableSource<T>` contract is being removed rather than
  preserved as a parallel abstraction.
- Relm4 boilerplate should be hidden behind procedural macros where practical.
- Blocking source work must stay outside the GTK UI thread.
- Styling belongs in external CSS files, not hardcoded Rust widget properties.

## Target Workspace Layout

```text
rsynapse/shell/
├── Cargo.toml
├── core/
│   ├── shell-core/        # package: shell-core
│   ├── background-effect/ # package: gtk4-background-effect
│   ├── macros/            # package: shell-macros
│   └── rx-macros/         # package: shell-rx-macros
├── app/                   # package: rsynapse-shell
└── launcher/              # launcher workspace, to be renamed/split later
```

This repository is the shell UI monorepo. Reusable framework crates live under
`core/`; concrete Rsynapse UI behavior lives in `app/`, `launcher/`, or future
surface crates such as `bar/`, `osd/`, and `notifications/`. Framework crates
must not take product-specific policy from the app or launcher.

## Runtime Architecture

Locus Shell widgets are thin UI processes. They subscribe to typed source
expressions, translate source updates into Relm4 messages, and let Relm4 update
watched GTK properties. D-Bus-backed sources should receive updates through
signals and property change streams; custom sources use the same Observable
binding path.

```text
+---------------+                    +------------------+                    +-----------------+
| D-Bus service | --zbus streams---->| RxRust Observable| --Injected Msg---->|   Relm4 Model   |
| properties    |                    | source expression|                    | (State Mutate)  |
+---------------+                    +------------------+                    +-----------------+
                                                                                      |
                                                                               Triggers #[watch]
                                                                                      |
                                                                             +--------v--------+
                                                                             |  GTK4 Widget    |
                                                                             +-----------------+
```

The client should not poll or diff large payloads. Source services own their
objects and signals; the shell Observable layer composes updates delivered by
source functions.

## Crates

### `shell-core`

Common UI support crate for GTK4/Relm4 widgets.

Responsibilities:

- Provide a generic process-level app wrapper for GTK/Relm4 shell widget binaries.
- Register global CSS/SCSS stylesheets and optional development-time stylesheet watchers.
- Create GTK windows with explicit layer-shell options.
- Encapsulate setup for GTK4 layer-shell integration.
- Offer small abstractions for raw layer-shell configuration: anchors, surface margins, exclusive zones, layers, namespace, and keyboard mode.
- Support fixed and automatic exclusive zones; automatic exclusivity reserves compositor space from the layer surface's computed size.
- Avoid consumer roles such as panel, bar, overlay widget, notification, or OSD. Those roles belong to consuming crates.

Non-responsibilities:

- No product-specific shell widgets.
- No panel/bar/OSD constructors.
- No product-specific application policy beyond generic lifecycle setup.
- No D-Bus subscription policy.
- No visual styling content; consumers provide stylesheets.

Initial dependencies:

- `gtk4`
- `relm4`
- `gtk4-layer-shell`, or another GTK4-compatible Wayland layer shell binding

### `shell-macros`

Procedural macro crate that reduces Relm4 widget boilerplate and binds UI state
to typed sources.

Initial dependencies:

- `syn`
- `quote`
- `proc-macro2`

Responsibilities:

- Parse `#[shell_macros::component(...)]` attributes stacked with `#[relm4::component]`.
- Parse typed state models annotated with `#[shell_macros::model]`.
- Extract typed source expressions from model fields of the form:

```rust
#[shell_macros::model]
pub struct Bar {
    #[source(selected_window_title())]
    pub selected_window_title: String,
}
```

- Keep `#[shell_macros::component(model = Bar)]` focused on Relm4 lifecycle wiring and view tracking.
- Preserve legacy component-level bindings during the transition:

```rust
selected_window_title: String = selected_window_title()
```

- Generate model state for resolved values.
- Generate message handling for field updates.
- Generate async subscription setup that forwards source updates into Relm4
  input messages.
- Support binding sources:
  - D-Bus property, signal, and ObjectManager expressions through observable
    source functions.
  - Consumer-defined D-Bus or custom service helpers.
  - Consumer-defined observable source functions declared with
    `#[shell_macros::observable]`.
- Keep `#[source(...)]` on model fields as the single syntax for binding a
  plain model value from a source expression.
- Add derived-source function support as described in `SOURCE_API.md`: function
  args use `#[observe(...)]` for observable dependencies and `#[inject]` for
  stable DI services.
- Rewrite `#[bind(field)]` view setters into Relm4 `#[track(...)]` updates so only widgets bound to the changed field redraw. `#[locus(...)]` remains a compatibility spelling during the transition.

Target authoring shape:

```rust
#[shell_macros::model]
pub struct Bar {
    #[source(selected_window_title())]
    pub selected_window_title: String,
    #[source(DISPLAY_DEVICE.bind(DisplayDevice::PERCENTAGE))]
    pub battery_percent: f64,
}

#[shell_macros::component(model = Bar)]
#[relm4::component(pub)]
impl SimpleComponent for Bar {
    type Input = sources::Msg;

    view! {
        gtk::Window {
            gtk::Label {
                #[bind(selected_window_title)]
                set_label: |title| title.as_str(),

                #[bind(selected_window_title)]
                set_css_classes: window_title_classes,
            }
        }
    }
}
```

Generated concepts:

- A `sources` module scoped beside the component by default.
- A user-authored state struct containing one field per typed binding.
- One private generated `__shell` runtime field for dirty tracking, last errors, and subscription ownership.
- Unified update message with one generated variant per field.
- An initialization hook that starts source subscriptions and emits Relm4
  messages.
- Per-field dirty tracking, cleared after Relm4 updates the view.
- View setter adapters that receive typed references to generated model fields.
- Dynamic styling through normal GTK setters such as `set_css_classes`; CSS contents still live in external stylesheets.

### `shell-core::source`

`source` is the small shell-owned RxRust facade used by generated code and
consumer sources.

Responsibilities:

- Re-export `rxrust` and the shell-owned `Observable<T, E = String>` alias.
- Expose reusable Observable source primitives for D-Bus properties, signals,
  ObjectManager object lists, and consumer-defined source functions.
- Keep backend-specific clients behind source implementation files.

D-Bus source binding is owned by Observable source helpers. Consumer crates
compose typed service helpers with shell-core source primitives; the framework
does not expose product-specific object descriptors.

### `shell-rx-macros`

Small declarative macros for RxRust composition ergonomics.

Responsibilities:

- Expand to normal RxRust operator chains without introducing a runtime layer.
- Keep heterogeneous source composition concise where RxRust exposes only
  binary operators, for example `combine_latest!`.
- Stay independent from product-specific widgets and backend transports.

### User-facing UI crates

User-facing shell crates or binaries such as bars, OSDs, and notifications.

Responsibilities:

- Live outside the core framework boundary, but inside the shell monorepo.
- Decide whether to run one or more Relm4 applications/processes.
- Load their own CSS.
- Decide their own shell roles, placement, exclusive zones, and behavior.
- Use `shell-core` only for generic layer-shell setup.

## Implementation Status

`PLAN.md` is the live roadmap. At a high level, the current workspace already has:

- `core/shell-core` for app startup, CSS/SCSS loading, and generic layer-shell windows.
- `core/background-effect` for reusable GTK4 `ext-background-effect-v1`
  background blur setup.
- `core/macros` for Relm4 source/model bindings.
- `core/rx-macros` for lightweight RxRust composition macros.
- `shell_core::source` for the current Observable binding facade.
- `SOURCE_API.md` for the Observable-first source API.
- `app/` for the current combined `rsynapse-shell` bar, OSD, notifications,
  request bridge, and styles.
- `launcher/` for the current launcher workspace.

Future user-facing widgets such as separate bar and OSD crates should be
created beside `app/` and consume the framework crates from `core/`.

## Engineering Guardrails

- Do not block the GTK UI thread with source watch/read work.
- Use Observable subscriptions for reactive work; source expressions own their
  async/blocking policy.
- Avoid allocations in render/watch paths where practical.
- Prefer `as_str()` and precomputed model state over `format!` inside `#[watch]`.
- Keep CSS in stylesheet files and attach classes from Rust.
- Avoid hardcoded visual styling in Rust.
- Keep each widget binary independently runnable.
- Keep macro output understandable enough to debug with `cargo expand`.
- Add tests around macro parsing before expanding generated behavior.
- Keep model fields as plain values. Observable and DI machinery belongs in
  generated source wiring, not in Relm4 component state.

## Open Integration Questions

- Exact shape of the existing `locus-dbus` Resolve proxy API.
- Whether the final workspace root is this repository or a parent `locus` workspace.
- Which GTK4 layer-shell crate is actively maintained and compatible with the target platform.
- Whether async subscriptions should settle on Tokio, GLib, or a thin compatibility layer.
- The final D-Bus payload format for `ResolveChanged` values.
