"""ComponentHealth (one health node) + the synchronous HealthIndicator contract."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Dict

from .status import HealthStatus


@dataclass
class ComponentHealth:
    status: HealthStatus
    details: Dict[str, Any] = field(default_factory=dict)
    components: Dict[str, "ComponentHealth"] = field(default_factory=dict)

    @classmethod
    def up(cls) -> "ComponentHealth":
        return cls(HealthStatus.UP)

    @classmethod
    def unknown(cls) -> "ComponentHealth":
        return cls(HealthStatus.UNKNOWN)

    @classmethod
    def down(cls, reason: str) -> "ComponentHealth":
        return cls(HealthStatus.DOWN, {"error": reason})

    @classmethod
    def degraded(cls, reason: str) -> "ComponentHealth":
        return cls(HealthStatus.DEGRADED, {"error": reason})

    def with_detail(self, key: str, value: Any) -> "ComponentHealth":
        self.details[key] = value
        return self

    def with_subcomponent(self, name: str, child: "ComponentHealth") -> "ComponentHealth":
        self.components[name] = child
        return self

    def effective_status(self) -> HealthStatus:
        acc = self.status
        for c in self.components.values():
            acc = acc.worse(c.effective_status())
        return acc

    def to_json(self) -> str:
        from .serialize import component_to_json

        return component_to_json(self)


class HealthIndicator:
    """A synchronous health probe. Subclass and override, or use ``fn_indicator``."""

    def name(self) -> str:  # pragma: no cover - abstract
        raise NotImplementedError

    def check(self) -> ComponentHealth:  # pragma: no cover - abstract
        raise NotImplementedError


class _FnIndicator(HealthIndicator):
    def __init__(self, name: str, fn: Callable[[], ComponentHealth]) -> None:
        self._name = name
        self._fn = fn

    def name(self) -> str:
        return self._name

    def check(self) -> ComponentHealth:
        return self._fn()


def fn_indicator(name: str, fn: Callable[[], ComponentHealth]) -> HealthIndicator:
    return _FnIndicator(name, fn)
