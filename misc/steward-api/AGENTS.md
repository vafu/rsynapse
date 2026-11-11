# steward-api

Typed D-Bus contract for Steward associations.

- Contract only; implementation lives in `misc/steward`.
- Keep it independent of shell UI and service internals.
- Consumers access it through `apps/shared/data-sources`.
- `RemarkedUiProxy` is the contract for asking the remarked UI to select a
  notebook. The remarked daemon's `org.remarked.Notebooks1` does not own an
  active notebook; selection is per UI window.
