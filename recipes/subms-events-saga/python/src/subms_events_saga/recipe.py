"""Perf runner emitting the SubMsBenchSummary JSON contract (lang=python).
Stages: build / commit / compensate. Run:

    PYTHONPATH="src;../../subms-events/python/src" python -m subms_events_saga.recipe > ../perf/python.json
"""

from __future__ import annotations

import json
import sys
import time

from .saga import Saga

STEPS = 8


def _ok():
    return None


def _boom():
    raise Exception("boom")


def _commit_saga() -> Saga:
    s = Saga("bench")
    for i in range(STEPS):
        s.step(f"s{i}", _ok, _ok)
    return s


def _fail_saga() -> Saga:
    s = Saga("bench")
    for i in range(STEPS - 1):
        s.step(f"s{i}", _ok, _ok)
    s.step("last", _boom, _ok)
    return s


def _summarize(samples):
    s = sorted(samples)
    n = len(s)

    def pct(p):
        return s[min(n - 1, int(p * n))]

    step = max(1, n // 500)
    return {
        "count": n,
        "p50_ns": pct(0.50),
        "p99_ns": pct(0.99),
        "p999_ns": pct(0.999),
        "max_ns": s[-1],
        "mean_ns": int(sum(s) / n),
        "samples_ns": s[::step][:500],
    }


def run(entries: int = 20_000) -> dict:
    build = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        saga = _commit_saga()
        build.append(time.perf_counter_ns() - t0)

    csaga = _commit_saga()
    commit = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        csaga.run()
        commit.append(time.perf_counter_ns() - t0)

    fsaga = _fail_saga()
    comp = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        fsaga.run()
        comp.append(time.perf_counter_ns() - t0)

    return {
        "workload": "subms-events-saga",
        "lang": "python",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "inputs": {"entries": str(entries)},
        "meta": {
            "subms.recipe.slug": "subms-events-saga",
            "subms.recipe.category": "concurrency",
            "steps": str(STEPS),
            "subms.workload.feature": "in-process-compensation",
        },
        "stages": {
            "build": _summarize(build),
            "commit": _summarize(commit),
            "compensate": _summarize(comp),
        },
    }


def main() -> None:
    entries = int(sys.argv[1]) if len(sys.argv) > 1 else 20_000
    json.dump(run(entries), sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
