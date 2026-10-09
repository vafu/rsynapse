# Dashboard client boundary

This is a local HTTP/SSE and Grafana UI adapter, not the project-management
system. `projd` owns project and goal records; call its D-Bus API for CRUD and
observe changes. Locusd is only for associations managed by steward/integration.

Keep private titles/criteria in domain records, not Graphite metric names.
Steward handles association mutation requests; locus is queried for resolution.
Metric configuration and visualization are UI/integration concerns. Never add
a second project/task database or make projd import Grafana/shell APIs.
