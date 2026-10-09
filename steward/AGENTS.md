# Steward side-effect boundary

Steward is one binary of small shell-source based listener/side-effect handlers:
metrics, derived naming, association policy, and domain refresh triggers. Keep
handlers as ordinary modules; do not build a dynamic plugin framework by default.

Steward owns Rsynapse association decisions and writes them to locusd: workspace
to project, window to project, agent to window/project. Services
remain authoritative for their properties. Locusd resolves and streams links;
it is not a project database. Explicit persistence is required for durable links.

Projd owns project metadata, checkouts and goals. Steward invokes its public API
and observes it through shell-source; never own duplicate project/goal CRUD.
Agents are clients of projd, not inferred goal-completion events.

Steward owns activity measurements, workday metrics and session-side policy.
It writes standard Carbon plaintext to the separate Graphite service. Grafana
deployment, dashboards and editor panels do not belong in this binary/crate.
Do not add a project-management store or planning UI here.
