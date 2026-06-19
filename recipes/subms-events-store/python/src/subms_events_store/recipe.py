"""Perf runner emitting the SubMsBenchSummary JSON contract (lang=python).
Stages: append / replay / catch_up. Run:

    PYTHONPATH="src;../../subms-events/python/src" python -m subms_events_store.recipe > ../perf/python.json
"""

from __future__ import annotations

import json
import sys
import time

from subms_events import Event

from .event_store import EventStore
from .projection import Projector, replay

REPLAY_N = 1_000


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
    store = EventStore()
    append = []
    for i in range(entries):
        ev = Event.builder("evt").at("t").attr("i", str(i)).build()
        t0 = time.perf_counter_ns()
        store.append(ev)
        append.append(time.perf_counter_ns() - t0)

    base = EventStore()
    for _ in range(REPLAY_N):
        base.append(Event.builder("evt").at("t").build())
    rep = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        replay(base, 0, lambda n, e: n + 1)
        rep.append(time.perf_counter_ns() - t0)

    proj = Projector(0)
    proj.catch_up(base, lambda n, e: n + 1)
    cu = []
    for _ in range(entries):
        base.append(Event.builder("x").at("t").build())
        t0 = time.perf_counter_ns()
        proj.catch_up(base, lambda n, e: n + 1)
        cu.append(time.perf_counter_ns() - t0)

    return {
        "workload": "subms-events-store",
        "lang": "python",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "inputs": {"entries": str(entries)},
        "meta": {
            "subms.recipe.slug": "subms-events-store",
            "subms.recipe.category": "storage",
            "replay_window": str(REPLAY_N),
            "subms.workload.feature": "in-memory-event-sourcing",
        },
        "stages": {
            "append": _summarize(append),
            "replay": _summarize(rep),
            "catch_up": _summarize(cu),
        },
    }


def main() -> None:
    entries = int(sys.argv[1]) if len(sys.argv) > 1 else 20_000
    json.dump(run(entries), sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
