# Shared observability deployment

Graphite and Grafana are independent services. Steward writes the existing
Carbon plaintext protocol on 127.0.0.1:2003; there is no replacement metrics API.
Grafana and its editor panels are clients of domain services via dashboard-api.

```sh
docker compose -f observability/docker-compose.yml up -d
```

The Compose project remains `metrics`, preserving its named storage volumes.
Dashboard files, color registry and Graphite retention aggregation live here,
outside steward and projd. Projd has no dependency on this deployment.
