"""Framework-agnostic endpoint helper. Returns (status_code, json_str) from the
cached snapshot - wire it into FastAPI / Flask / Django yourself."""

from __future__ import annotations

from typing import Optional

from .registry import HealthRegistry, ProbeKind


def health_endpoint(registry: HealthRegistry, probe: Optional[ProbeKind] = None) -> tuple:
    if probe == ProbeKind.LIVENESS:
        return registry.render_liveness()
    if probe == ProbeKind.READINESS:
        return registry.render_readiness()
    if probe == ProbeKind.STARTUP:
        return registry.render_startup()
    return registry.render()
