# AGS Shell Reference

This document summarizes `/home/v47/.config/ags` as a product reference for future Rust shell widgets. It is not an implementation template. The AGS code uses JavaScript/TypeScript, Astal, RxJS-style streams, and imperative GTK updates; `locus-shell` should use the feature inventory and data contracts while keeping the Rust/Relm4 architecture typed, compiled, and declarative.

## Top-Level Shape

The AGS app starts one bar window per monitor and creates additional overlays such as OSD, search, and agent approval windows on the active monitor. The bar is a layer-shell window anchored along an edge with exclusivity. This confirms the framework boundary: `shell-core` should make layer-shell windows and CSS loading easy, while actual bar layout and behavior belong to a consumer crate.

## Bar Layout

The bar is split into three regions:

- Left: workspace/project status and build status.
- Center: active workspace window indicators.
- Right: media, system stats, tray, power, Bluetooth, audio, network, battery, and clock.

For Rust this suggests a future external `locus-bar` crate, not framework code. The framework should provide provider/macros/core primitives that make this composition ergonomic.

## Data Sources

Relevant source families:

- Locus graph D-Bus: workspaces, windows, projects, app instances, outputs, selected entities, build invocations.
- Agent D-Bus: agent sessions, status, pending prompts, model/cost/context details, approval interactions.
- Standard D-Bus/system services: UPower battery, NetworkManager or equivalent, Bluetooth, notifications, tray/status notifier.
- PipeWire/WirePlumber: audio sinks/sources, default route, volume/mute.
- MPRIS: media player metadata and playback status.
- Process/file sources: sysstats scripts, CSS compilation, theme/accent sync.
- Time sources: clock and date ticks.
- HTTP/cache sources: weather and icon enrichment can be derived from Locus/system state.

## Domain Models

The AGS bar builds summarized data before rendering. Equivalent Rust models should be explicit structs emitted by generated sources or user-authored observable source functions:

- `WorkspaceStatus`: workspace id/index/name, active/current/urgent state, project display data, work status.
- `WindowIndicator`: window id/title/app icon, active/urgent state, agent session linkage, compact status.
- `AgentStatus`: session id, branch, cwd, model, token/context usage, cost, task state, attention/prompt state.
- `BuildStatus`: offline/idle/running/failed/finished, current invocation, recent failures, tooltip details.
- `SystemStatus`: battery, network, Bluetooth, power profile, audio route, tray items, media, clock.

## Update Pattern To Preserve

AGS uses RxJS-style streams with `shareReplay`, distinct checks, and per-widget subscriptions. The Rust equivalent should preserve that dataflow clarity through the Observable source API in `SOURCE_API.md`, not through JavaScript or imperative GTK wiring. It should use:

- source expressions that emit typed values,
- derived observable source functions for summaries,
- Relm4 messages for model updates,
- `#[bind(field)]` tracked setters for precise GTK updates,
- shared source/subscription handles to avoid duplicate D-Bus watches.

The key product behavior is “derive summarized UI models from multiple sources”; the implementation should stay Rust-native and macro-driven.

## Styling Reference

Styling is external SCSS/CSS. The bar uses class-driven states for active, urgent, attention, working, complete, agent status, build status, and level severity. `shell-core` should continue to own generic CSS/SCSS loading only; consumer crates own class names and visual policy.

## Feature Inventory For Future Crates

Likely reusable crates or feature-gated modules:

- Observable source API: `SOURCE_API.md`.
- Current `providers`: migration bridge for source traits, subscriptions, and sharing.
- `locus-provider`: generic Locus graph watch implementation used by generated schema sources.
- `dbus-provider`: generic D-Bus object/property source implementation.
- `common-providers`: feature-gated typed definitions for common services such as UPower.
- Future `pipewire` or `common-pipewire`: typed audio sources.
- Future `common-system`: time, sysstats, filesystem, process sources.
- Future `common-network`: network and Bluetooth sources if D-Bus definitions grow large.
- Future `common-media`: MPRIS and status-notifier/tray helpers.
- Future `common-icons`: icon lookup/cache sources.

## Boundary

The AGS bar behavior is the end-goal reference for a user-facing shell, not for `shell-core`. The framework should provide the toolkit to build this kind of shell easily in Rust; it should not embed product-specific widgets or policies.
