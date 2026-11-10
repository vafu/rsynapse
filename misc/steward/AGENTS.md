# Steward

Single-binary session listener/side-effect host for metrics, naming,
association policy, and refresh triggers.

- Observe services through `data-sources`; do not duplicate project/goal CRUD.
- Steward owns association decisions written to locusd; services remain
  authoritative for their own properties.
- Compositors own workspace/window topology. Steward expires stale associations
  and performs placement only for explicit restore requests.
- Metrics use standard Carbon plaintext to Graphite. Grafana deployment stays in
  `misc/observability`.
- This directory is part of `misc/` and is intentionally unsettled.
