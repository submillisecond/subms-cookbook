"""Perf runner emitting the SubMsBenchSummary JSON contract (lang=python).
Stages: register / report / render_json. Run:

    PYTHONPATH="src;../../subms-events/python/src" python -m subms_health.recipe > ../perf/python.json
"""

from __future__ import annotations

import json
import sys
import time

from .component import ComponentHealth
from .env_section import EnvSection, MapEnv
from .registry import HealthConfig, HealthRegistry, RefreshPolicy

INDICATORS = 16
ENV_VARS_PER_SECTION = 8


def _build_registry() -> HealthRegistry:
    reg = HealthRegistry(HealthConfig.sync())
    for i in range(INDICATORS - 2):
        if i % 5 != 0:
            reg.register_fn(
                f"dep-{i}",
                lambda: ComponentHealth.up().with_detail("ping", "ok"),
                RefreshPolicy(interval_ms=1_000),
            )
        else:
            reg.register_fn(
                f"dep-{i}",
                lambda: ComponentHealth.degraded("slow upstream"),
                RefreshPolicy(interval_ms=1_000),
            )
    for s in range(2):
        env = MapEnv()
        for v in range(ENV_VARS_PER_SECTION):
            env.with_(f"KICKSTART_VAR{s}_{v}", f"value-{s}-{v}")
        section = (
            EnvSection(f"deploy-{s}")
            .prefix("KICKSTART_")
            .strip_prefix_in_key(True)
            .lowercase_keys(True)
            .redact_secrets()
        )
        reg.register(section.into_indicator(env), RefreshPolicy(interval_ms=60_000, critical=False))
    return reg


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
    register = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        reg = _build_registry()
        register.append(time.perf_counter_ns() - t0)

    reg = _build_registry()
    report = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        reg.refresh_now()
        report.append(time.perf_counter_ns() - t0)

    reg.refresh_now()
    render = []
    for _ in range(entries):
        t0 = time.perf_counter_ns()
        reg.render()
        render.append(time.perf_counter_ns() - t0)

    return {
        "workload": "subms-health",
        "lang": "python",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "inputs": {"entries": str(entries)},
        "meta": {
            "subms.recipe.slug": "subms-health",
            "subms.recipe.category": "observability",
            "indicators": str(INDICATORS),
            "env_vars_per_section": str(ENV_VARS_PER_SECTION),
            "subms.workload.feature": "cached-snapshot-render",
        },
        "stages": {
            "register": _summarize(register),
            "report": _summarize(report),
            "render_json": _summarize(render),
        },
    }


def main() -> None:
    entries = int(sys.argv[1]) if len(sys.argv) > 1 else 20_000
    json.dump(run(entries), sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
