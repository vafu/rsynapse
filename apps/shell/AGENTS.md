# Shell App

Main bar, OSD, workspace selector, and media overlay.

- Widgets compose typed Observables from `apps/shared/data-sources`.
  Never construct `zbus` clients or spawn service/process readers here.
- Follow `core/shell-core/CORE_WIDGETS_GUIDE.md`: use Relm4 and shell macros;
  reconcile list structure by stable keys and subscribe per item.
- App startup, tracing, and the request socket come from `app-kit`.
- Keep product UI under `widgets/`; reusable framework behavior belongs in
  `core/shell-core` and reusable service access in `data-sources`.
- See `src/widgets/AGENTS.md` for widget-local rules.
