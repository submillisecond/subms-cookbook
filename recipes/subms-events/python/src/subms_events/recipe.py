"""Perf runner emitting the SubMsBenchSummary JSON contract (lang=python) for the
cookbook. Stages: build / emit_sync / emit_async. Run:

    PYTHONPATH=src python -m subms_events.recipe > ../perf/python.json
"""

from __future__ import annotations

import json
import sys
import time

from .dispatcher import EventDispatcher
from .event import Event, EventLevel
from .listener import listener


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


def run(entries: int = 50_000) -> dict:
    build = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        e = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN")
        build.append(time.perf_counter_ns() - t0)

    ev = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN")

    sync_bus = EventDispatcher.sync()
    sync_bus.add_listener(listener(lambda _e: None))
    sync = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        sync_bus.emit(ev)
        sync.append(time.perf_counter_ns() - t0)

    async_bus = EventDispatcher.asynchronous()
    async_bus.add_listener(listener(lambda _e: None))
    asy = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        async_bus.emit(ev)
        asy.append(time.perf_counter_ns() - t0)
    async_bus.stop()

    return {
        "workload": "subms-events",
        "lang": "python",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "inputs": {"entries": str(entries)},
        "meta": {
            "subms.recipe.slug": "subms-events",
            "subms.recipe.category": "concurrency",
            "subms.workload.feature": "in-process-dispatch",
        },
        "stages": {
            "build": _summarize(build),
            "emit_sync": _summarize(sync),
            "emit_async": _summarize(asy),
        },
    }


def main() -> None:
    entries = int(sys.argv[1]) if len(sys.argv) > 1 else 50_000
    json.dump(run(entries), sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
