# Observability boundary

Own shared Graphite/Grafana deployment, dashboards and visualization here.
Use existing Carbon/Graphite protocols. Steward emits metrics; projd owns domain
records; UI adapters consume them. Never add Grafana imports or lifecycle to
projd, or metrics storage to steward. Preserve the `metrics` volume identities.
