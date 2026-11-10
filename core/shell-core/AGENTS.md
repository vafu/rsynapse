# shell-core

Generic GTK/Relm4 integration used by all apps.

- Owns app startup, model/list helpers, window/layer-shell setup, CSS loading,
  and the `SourceModel` contract used by macros.
- Widgets compose `data-sources` Observables; framework code stays service-free.
- Preserve list reconciliation semantics: structure changes rebuild rows, item
  updates subscribe per row.
- See `CORE_WIDGETS_GUIDE.md` for the canonical reactive UI patterns.
