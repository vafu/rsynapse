# Shared observability deployment

Graphite and Grafana are independent services. Steward writes the existing
Carbon plaintext protocol on 127.0.0.1:2003; there is no replacement metrics API.
Grafana's goal metrics and workday settings panels are clients via dashboard-api.

```sh
docker compose -f observability/docker-compose.yml up -d
```

The Compose project remains `metrics`, preserving its named storage volumes.
Dashboard files, color registry and Graphite retention aggregation live here,
outside steward and projd. Projd has no dependency on this deployment.

The **Rsynapse goal work metrics** dashboard is provisioned at
http://localhost:3000/d/rsynapse-goals/rsynapse-goals. Its local panel reads
goal-instance history and work spans through dashboard-api, with Grafana range
selection and native refresh. Focus overview and productivity dashboards link to
it. Goal data remains authoritative in projd SQLite; Graphite continues serving
the existing aggregate metric series.

The productivity dashboard includes **Workday target settings**, a standalone
span/unlocked/active preference editor. Goal/task management lives in `proj goal`
and the shell checklist. The former daily-goal Grafana editor and its HTTP CRUD
routes have been retired; existing goal records and metrics history remain in projd.
