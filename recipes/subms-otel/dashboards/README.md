# subms-otel dashboards

Pre-built Grafana boards + Prometheus alerts for the `subms-otel` bridge.

```
dashboards/
  grafana/
    subms-overview.json      # p99 + ops/sec + in-flight + drops + divergence
    subms-tail-debug.json    # latency heatmap + exemplar table
  prometheus/
    alerts/
      subms-p99.yaml         # p99 > 1ms, divergence > 0, drops > 0
```

## Metric-name convention

Both boards assume the OTel Collector's prometheus exporter (or the SDK's
prometheus exporter) is translating OTel names to Prometheus shape:

| OTel name                          | Prometheus shape                                                  |
|------------------------------------|-------------------------------------------------------------------|
| `subms.latency` histogram, unit `s` | `subms_latency_seconds_bucket` / `_count` / `_sum`                |
| `subms.bench.ops_total` counter    | `subms_bench_ops_total`                                           |
| `subms.bench.in_flight` gauge      | `subms_bench_in_flight`                                           |
| `subms.otel.dropped_total` counter | `subms_otel_dropped_total`                                        |
| `subms.otel.exemplars_kept_total`  | `subms_otel_exemplars_kept_total`                                 |
| `subms.reference.divergence` ctr   | `subms_reference_divergence_total`                                |
| attribute `subms.recipe.slug`      | label `subms_recipe_slug` (dots to underscores)                   |

If your pipeline differs, find-and-replace the metric names in the JSON +
YAML before importing.

## Import the Grafana boards

Via UI:

1. Grafana -> Dashboards -> New -> Import.
2. Upload `grafana/subms-overview.json`. Pick your Prometheus data source.
3. Repeat for `grafana/subms-tail-debug.json`.

Via API:

```sh
curl -X POST \
  -H "Authorization: Bearer $GRAFANA_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$(jq -c '{dashboard: ., overwrite: true}' grafana/subms-overview.json)" \
  https://grafana.example.com/api/dashboards/db

curl -X POST \
  -H "Authorization: Bearer $GRAFANA_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$(jq -c '{dashboard: ., overwrite: true}' grafana/subms-tail-debug.json)" \
  https://grafana.example.com/api/dashboards/db
```

Or via the Grafana provisioning files setup: drop both JSON files into a
`provisioning/dashboards/subms/` directory and point a provider at it.

## Apply the Prometheus alerts

Place the rules file where your Prometheus / Mimir / Cortex config picks
it up:

```yaml
# prometheus.yml
rule_files:
  - "alerts/subms-p99.yaml"
```

Reload Prometheus (`curl -X POST http://prom:9090/-/reload`) or restart.

For Kubernetes via the `kube-prometheus-stack` Helm chart, wrap the rules
in a `PrometheusRule` CR:

```sh
kubectl create configmap subms-alerts \
  --from-file=prometheus/alerts/subms-p99.yaml \
  --namespace monitoring
```

## Linking exemplars to traces

The tail-debug board's exemplar table includes a column-level link that
points at Tempo by default. Edit the `links.url` field in
`subms-tail-debug.json` to point at your tracing backend (Jaeger, Honeycomb,
Datadog, etc.) - the `${__value.raw}` placeholder substitutes the
`trace_id`.

For exemplars to populate, the histogram must be scraped with `?exemplars=true`
(Prometheus 2.43+) and the `tracing` feature of `subms-otel` must be on so
the W3C TraceContext attaches a real `trace_id` to each recorded sample.

## What the alerts mean

| Alert                            | Fires when                                                  | Action                                                                          |
|----------------------------------|-------------------------------------------------------------|---------------------------------------------------------------------------------|
| `SubmsRecipeP99OverBudget`       | 5m p99 > 1ms on a recipe + stage                            | Open the overview board, scope to that recipe, check ops/sec for load spike     |
| `SubmsReferenceDivergence`       | Any non-zero rate on `subms.reference.divergence`           | Correctness drift - escalate immediately; perf is fine but answers are wrong    |
| `SubmsAsyncObserverDropping`     | `subms.otel.dropped_total` rate > 0 sustained 1m            | Raise `SUBMS_OTEL_ASYNC_CAPACITY` or accept drops as back-pressure signal       |
| `SubmsAsyncObserverInFlightSaturating` | in-flight gauge approaches ring cap                  | Pre-saturation warning - tune capacity before drops start                       |

## See also

- Primer: <https://submillisecond.com/cookbook/primers/subms-otel>
- Source: <https://github.com/submillisecond/subms-otel>
