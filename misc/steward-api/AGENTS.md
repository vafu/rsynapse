# steward-api

Typed D-Bus contract for Steward associations.

- Contract only; implementation lives in `misc/steward`.
- Keep it independent of shell UI and service internals.
- Consumers access it through `apps/shared/data-sources`.
