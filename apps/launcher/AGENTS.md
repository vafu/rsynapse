# Launcher

Launcher daemon, CLI, UI, plugin contract, and plugins.

- Plugins call `apps/shared/data-sources` for service state. Bluetooth, audio,
  and workspace plugins must not use `zbus` or spawn data-collection commands.
- Command/shell plugins may execute user-requested actions; those are actions,
  not observation. Keep that distinction explicit.
- Preserve launcher IDs, groups, plugin registration, hints, and
  `LauncherActivity` behavior when refactoring.
- Add blocking adapters to `data-sources` when the synchronous plugin API needs
  them; do not duplicate protocol code in the plugin.
