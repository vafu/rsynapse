# Shared App Crates

Shared application utilities. Keep product-specific policy in the app crates.

## data-sources

- The only app-facing service-data layer. UI widgets and launcher plugins call
  typed adapters here; they do not construct `zbus` proxies or commands.
- Adapters may expose reactive Observables and explicit blocking/async writes
  where a consumer API requires them.
- Keep service ownership clear: `proj`, `workspace`, `workspace_project`,
  `mpris`, `bluetooth`, `audio`, `github`, `mist`, `systray`, and similar modules
  wrap their owning service contracts.
- App-owned D-Bus servers (for example the notification daemon interface) stay
  in their app; this crate observes external data.

## app-kit

- Generic GTK/Relm4 startup, tracing/profiling, launch mode, and the request
  socket. It does not own product surfaces.

## desktop-entry / nerd-icon-picker

- Reusable parsing, launch, catalog, and icon utilities. Do not add service
  protocol or shell policy here.
