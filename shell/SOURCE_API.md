# Observable Source API Design

## Summary

The user-facing source API is Observable-first.

Widget models store plain values. Source expressions create typed observables.
Macros subscribe those observables and write emitted values into Relm4 model
fields.

```text
D-Bus source function / user source function
    -> Observable<T>
    -> shell macro subscription
    -> Relm4 Msg
    -> plain model field: T
```

`ObservableSource<T>` is not part of the target design. Source composition uses
RxRust operators and the shell-owned `Observable<T, E = String>` alias.

## Authoring Model

Model fields keep the existing binding syntax:

```rust
#[shell_macros::model]
pub struct ProjectLabel {
    pub workspace_id: u64,

    #[source(project_label(workspace_id))]
    pub label: ProjectLabelView,
}
```

The field type is the cached value type, not an observable type. The generated
sidecar module owns subscription lifecycle, error tracking, and dirty-field
updates.

Derived sources are ordinary Rust functions annotated with a source-definition
macro:

```rust
#[shell_macros::observable]
fn project_label(
    workspace_id: u64,

    #[observe(workspace_name(workspace_id))]
    workspace_name: Observable<String>,

    #[observe(workspace_project(workspace_id))]
    project: Observable<Option<String>>,

    #[observe(project_display_main(project.clone()))]
    project_name: Observable<Option<String>>,

    #[inject]
    theme: Arc<ThemeConfig>,
) -> Observable<ProjectLabelView> {
    Observable::combine_latest3(workspace_name, project, project_name)
        .map(move |(workspace_name, project, project_name)| ProjectLabelView {
            primary: project_name.unwrap_or(workspace_name),
            has_project: project.is_some(),
            accent: theme.project_accent(),
        })
}
```

The generated public function keeps only explicit call-time context parameters:

```rust
fn project_label(workspace_id: u64) -> Observable<ProjectLabelView>
```

Parameters are classified explicitly:

- Plain parameters are caller-provided context, such as a workspace id, window
  id, object path, or user option.
- `#[observe(expr)]` parameters are reactive observable dependencies created by
  generated code.
- `#[inject]` parameters are stable DI services resolved from the configured
  application dependency injector.

Do not use `#[source]` on derived-source function arguments. Reserve
`#[source(...)]` for model fields so the same word always means "bind this
model value from this source expression."

## D-Bus Sources

`shell_core::source::dbus` owns generic D-Bus primitives:

- `property(PropertyDescriptor)` for typed property values.
- `signal(SignalDescriptor)` for typed signal payloads.
- `object_manager(ObjectManagerDescriptor)` for managed object snapshots.

Consumer crates should wrap these primitives in typed service helpers. Widget
code should call semantic functions such as `selected_workspace_windows()` or
`active_power_profile()` rather than constructing raw descriptors in views.

Generic helpers should:

- Use `zbus` signals and property streams, not polling.
- Share active sources by a stable descriptor key through
  `source::shared_by_key`.
- Replay the latest value to late subscribers.
- Stop upstream work when the last subscriber drops.
- Keep D-Bus object paths typed with `zbus` path/name types at helper
  boundaries when practical.

## Macro Responsibilities

`#[shell_macros::model]`:

- Parses model-field `#[source(expr)]` attributes.
- Type-checks `expr` as an observable source for the field value type.
- Generates Relm4 message variants, dirty tracking, subscription startup, error
  storage, and cancellation ownership.
- Keeps the user model as plain state.

`#[shell_macros::observable]`:

- Parses a user function returning `Observable<T>`.
- Removes `#[observe]` and `#[inject]` parameters from the public call
  signature.
- Builds observed dependency expressions from the explicit context arguments
  and previously declared observed values.
- Resolves explicit DI services.
- Calls the user body with all parameters and returns the resulting observable.

`#[observe(expr)]`:

- Describes an observable dependency for a derived source function.
- May reference plain context parameters and earlier observed parameters.
- Supports dynamic dependencies such as `project_display_main(project.clone())`
  where `project` is itself `Observable<Option<String>>`; generated code should
  implement this as switch/restart behavior over the inner observable.

`#[inject]`:

- Resolves stable services from a DI layer, for example clients, config,
  loggers, theme policy, caches, or runtime handles.
- Does not inject reactive graph values. Reactive values use `#[observe]`.

## Observable Contract

The shell owns the public `Observable<T>` alias/re-export, backed by `rxrust`.
Consumer code should import the shell-owned name, but authoring should use
normal `rxrust` operators rather than shell-specific operator wrappers.

Required semantics:

- Shell-core uses RxRust's error channel for source failures.
- Macro-generated subscriptions turn terminal errors into model messages.
- Sources are shared and replay latest values by descriptor key where reuse is
  expected.
- Upstream work starts on first active subscription, stops when the last
  subscriber drops, and restarts for later subscribers.
- Cancellation must be cooperative and owned by generated subscription handles.

Useful operators for shell authors:

- `map`
- `filter_map`
- `distinct_until_changed`
- RxRust's binary `combine_latest`, plus `shell_rx_macros::combine_latest!`
  when fixed-arity heterogeneous composition would otherwise require repeated
  tuple mapper functions
- `switch_map` or equivalent dynamic dependency support
- `debounce` and throttling where widget behavior needs transient timing
- `Observable::create` for low-level custom sources

## DI Boundary

The Observable source API is DI-inspired, but it is not a general DI container.

Use DI for stable services:

- D-Bus clients and typed service proxies
- configuration and theme state
- caches and registries
- logging and metrics
- service-specific command clients

Use Observable source construction for dynamic values:

- D-Bus properties and object collections
- relation-service lookups
- timers, file watches, and process output
- derived UI DTOs from multiple sources

Use `nject` behind a small shell facade for stable services. The macro should
target shell-owned facade APIs, not `nject` internals, so service construction
can evolve without changing user source functions.

DI should not be responsible for reactive relation resolution, switch-map
behavior, subscription sharing, or Relm4 model updates.

## Migration Notes

The remaining migration work is:

1. Add `#[shell_macros::observable]`, `#[observe(...)]`, and `#[inject]` for
   user-authored derived sources.
2. Build typed D-Bus helper modules for Niri, AgentDBus, notifications, power,
   audio, media, and tray data as the corresponding services exist.
3. Replace placeholder app surfaces with source-bound widgets using those
   helpers.
4. Replace remaining ad hoc consumer source composition with annotated
   observable functions where that improves ergonomics.

During migration, do not add public `ObservableSource`-style composition APIs.
Composition belongs in `rxrust` operators and `#[shell_macros::observable]`
functions.
