# Apps

Concrete GTK/Relm4 applications and app-shared crates.

- `shell/`, `notifications/`, and `launcher/` are separate processes and may
  share UI components and data adapters, but not process state.
- `shared/data-sources/` is the only app-facing source of D-Bus, process, and
  service state. Add typed adapters there instead of using `zbus` in UI code.
- `shared/app-kit/` owns generic startup, tracing, launch mode, and the local
  request socket.
- `shared/desktop-entry/` and `shared/nerd-icon-picker/` are reusable app
  utilities.
- `examples/` is illustrative only; keep examples small and source-oriented.

Run `../scripts/check-data-access.sh` after changing any app data access.
