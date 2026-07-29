# subms-perf-gate (primer)

Runnable example for the [perf-regression gate primer](https://www.submillisecond.com/cookbook/primers/subms-perf-gate).

This primer ships no library - the artefact is the CI wiring itself:

- `.github/workflows/perf.yml` - the complete GitHub Actions pipeline (diff -> sink -> aggregate). Copy it into your repo and point the paths at your own `perf/*.json`.
- `.gitlab-ci.yml` - the GitLab CI sketch (the actions run there via `node diff.js`; a polished template is unscheduled).

The gate reads and writes the [`SubMsBenchSummary` JSON contract](https://github.com/submillisecond/subms-actions/blob/main/docs/JSON-CONTRACT.md), so any bench that emits the shape plugs in. The [`subms` harness](https://www.submillisecond.com/cookbook/primers/subms-perf-harness) emits it natively in Rust and Java.

Subject (the dep this primer demonstrates): the `submillisecond/subms-action-*` suite - `bench`, `diff`, `diff-aggregate`, `diff-sink`, `drift` - plus the `subms-actions` umbrella (reusable workflow + pre-commit hook).
