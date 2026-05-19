## Quickstart - GitHub Actions

Three lines of CI YAML give you a perf-regression status check on every PR.

```yaml
# .github/workflows/perf.yml
name: perf
on:
  pull_request:
    paths: ["**/perf/**.json"]

permissions:
  contents: read
  pull-requests: write

jobs:
  diff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 2 }

      - name: Snapshot baseline (base ref's perf JSON)
        run: git show ${{ github.event.pull_request.base.sha }}:perf/myworkload.rust.json > baseline.json

      - name: Use PR perf JSON
        run: cp perf/myworkload.rust.json candidate.json

      - uses: submillisecond/subms-action-diff@v1
        with:
          baseline: baseline.json
          candidate: candidate.json
          threshold-pct: "15"
          fail-on-regression: "true"
```

That's the whole gate. Any PR whose `perf/myworkload.rust.json` regresses beyond +15 % on any stage fails the status check. The action posts a sticky PR comment with the per-stage delta table.

## The five actions

Each lives in its own repo under `submillisecond/`. Reference by `@v1` (floating major tag) or pin to a release tag.

### `subms-action-bench` - run a bench, capture JSON

Wraps the "invoke bench / validate JSON / retry on flake / warmup" boilerplate.

```yaml
- uses: submillisecond/subms-action-bench@v1
  with:
    command: cargo run --release --features harness --example perf_main
    stdin: |
      entries=50000
      warmup=5000
    workdir: my-rust-crate
    warmup-runs: 1
    retries: 1
    output: perf.json
```

Validates the captured JSON shape (`workload`, `lang`, `stages.*.{p50_ns,p99_ns,...}`); rejects malformed output. JIT-heavy Java benches benefit from `warmup-runs: 2`; cold Rust benches don't need any.

### `subms-action-diff` - the gate

Compares two perf JSONs, posts a sticky PR comment, renders the diff inline in `$GITHUB_STEP_SUMMARY`, uploads the diff JSON as an artifact, and **fails the status check** on regression.

```yaml
- uses: submillisecond/subms-action-diff@v1
  with:
    baseline: baseline.json
    candidate: candidate.json
    threshold-pct: "15"
    per-stage-thresholds: |
      { "get_miss": 25, "warmup": 50 }
    fail-on-regression: "true"
    workload-label: "myworkload (rust)"
```

Per-stage thresholds let noisy stages (cold-cache, JIT warmup) tolerate more drift than hot-path stages.

### `subms-action-diff-aggregate` - matrix rollup

Running 32 jobs (16 components x 2 langs) would otherwise post 32 PR comments. The aggregator collects all `subms-diff-*` artifacts and posts **one** rollup comment with the worst-N regressing stages.

```yaml
aggregate:
  needs: diff
  if: always()
  runs-on: ubuntu-latest
  permissions:
    contents: read
    pull-requests: write
  steps:
    - uses: actions/checkout@v4
    - uses: actions/download-artifact@v4
      with:
        pattern: subms-diff-*
        path: subms-diffs
    - uses: submillisecond/subms-action-diff-aggregate@v1
      with:
        diff-glob: "subms-diffs/*"
        top-n: "12"
        fail-on-regression: "true"
```

The aggregate failure is what your branch protection rule should require - one stable check name across the whole matrix.

### `subms-action-diff-sink` - push to 13 observability sinks

Forwards the diff (or summary) JSON to downstream observability. Sinks:

| group | sinks |
|---|---|
| chat | `slack` |
| HTTP / REST | `http` (alias `rest`), `splunk`, `newrelic`, `honeycomb` |
| cloud storage | `s3`, `gcs`, `azure` (presigned URL PUT) |
| metrics | `prometheus`, `influxdb`, `datadog` |
| local | `file`, `stdout` |

Comma-separated list to push to several at once; per-sink failure is isolated.

```yaml
- uses: submillisecond/subms-action-diff-sink@v1
  if: steps.diff.outputs.has-regression == 'true'
  with:
    input: subms-diff.json
    sink: "slack,s3,datadog"
    webhook-url:      ${{ secrets.SLACK_WEBHOOK }}
    s3-url:           ${{ secrets.S3_PRESIGNED_URL }}
    datadog-api-key:  ${{ secrets.DATADOG_API_KEY }}
    only-on-regression: "true"
```

### `subms-action-drift` - rolling-window drift detection

Where `subms-action-diff` catches "this PR regressed vs the parent commit", `subms-action-drift` catches **"p99 has been creeping +1 % per week and is now 3.5 sigma above the 14-day mean"** - the slow case static base-ref diffing misses.

```yaml
- uses: submillisecond/subms-action-drift@v1
  with:
    candidate:     perf.json
    history-glob:  "history/myworkload-rust-*.json"
    metric:        p99
    k-stddev:      "3"
    min-history:   "10"
    window-size:   "30"           # last 30 nightly snapshots
    fail-on-drift: "true"
```

Welford's online algorithm; soft on missing history (informational comment, exit 0 below `min-history`).

## The reusable workflow - one call, whole pipeline

Don't want to wire five steps? The umbrella ships a reusable callable workflow:

```yaml
jobs:
  perf:
    uses: submillisecond/subms-actions/.github/workflows/subms-perf-suite.yml@v1
    secrets: inherit
    with:
      bench-command: "cargo run --release --features harness --example perf_main"
      baseline-source: "base-ref"
      baseline-path:   "perf/myworkload.rust.json"
      threshold-pct:   "15"
      sink:            "slack"
      drift-history-glob: "history/*.json"
```

Wraps bench -> diff -> sink -> drift. Inputs map to the five-action equivalents.

## Pre-commit hook - catch regressions before they leave your laptop

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/submillisecond/subms-actions
    rev: v0.1.0
    hooks:
      - id: subms-perf-diff
```

Diffs the staged perf JSON against the HEAD revision of the same file. Fast (<100 ms) - does NOT re-run the bench. Refuses the commit with override hints:

```text
subms-perf-diff: perf/myworkload.rust.json REGRESSED
  threshold=+10%
  put          p99        1.2us ->     1.5us     +25.0%  (threshold +10.0%)

Commit blocked. Override:
  SUBMS_PRECOMMIT_THRESHOLD_PCT=20 git commit ...
  SUBMS_PRECOMMIT_PER_STAGE='{"slow_path":25}' git commit ...
  SUBMS_PRECOMMIT_FAIL_ON_REGRESS=false git commit ...
  git commit --no-verify ...
```

## Enterprise

For corporate proxies / custom CAs / mTLS / OIDC / audit-trail / CODEOWNERS / GHES, see [`docs/Enterprise.md`](https://github.com/submillisecond/subms-actions/blob/main/docs/Enterprise.md) in the umbrella repo. The sinks honour `HTTPS_PROXY` / `NO_PROXY`, custom CA bundles (`SUBMS_CA_BUNDLE` or `NODE_EXTRA_CA_CERTS`), and mTLS client certs (`SUBMS_CLIENT_CERT` + `SUBMS_CLIENT_KEY`). Rate-limit-aware retry with exponential backoff is built in.
