"""Health status taxonomy + worst-wins aggregation."""

from __future__ import annotations

from enum import Enum
from typing import Iterable


class HealthStatus(Enum):
    UP = "UP"
    UNKNOWN = "UNKNOWN"
    WARN = "WARN"
    DEGRADED = "DEGRADED"
    DOWN = "DOWN"

    @property
    def rank(self) -> int:
        # Higher is worse: Up < Unknown < Warn < Degraded < Down.
        return {"UP": 0, "UNKNOWN": 1, "WARN": 2, "DEGRADED": 3, "DOWN": 4}[self.value]

    def worse(self, other: "HealthStatus") -> "HealthStatus":
        return other if other.rank > self.rank else self

    @staticmethod
    def aggregate(statuses: Iterable["HealthStatus"]) -> "HealthStatus":
        acc = HealthStatus.UP
        for s in statuses:
            acc = acc.worse(s)
        return acc


def http_status_for(status: HealthStatus) -> int:
    """UP/UNKNOWN/WARN -> 200, DEGRADED/DOWN -> 503."""
    if status in (HealthStatus.UP, HealthStatus.UNKNOWN, HealthStatus.WARN):
        return 200
    return 503
