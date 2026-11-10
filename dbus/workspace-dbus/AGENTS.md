# workspace-dbus

Compositor-neutral workspace projection.

- `api/` is the independent consumer contract.
- `service/` implements Niri and Umbriel backends; consumers must not depend on
  compositor protocols.
- Preserve `ObjectManager`, stable IDs, focus/workspace/window properties, and
  explicit control methods.
- App clients consume the API through `apps/shared/data-sources`.
