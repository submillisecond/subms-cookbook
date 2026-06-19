"""The registry: indicators + per-indicator policy, an optional background
refresher thread that probes off the request path, and a pre-rendered cached
snapshot served by render(). Status changes emit subms-events events."""

from __future__ import annotations

import os
import socket
import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable, Dict, List, Optional

from subms_events import DispatchMode, Event, EventBridge, EventDispatcher, EventLevel, EventListener

from .clock import Clock, SystemClock
from .component import ComponentHealth, HealthIndicator, fn_indicator
from .env_section import EnvSection
from .serialize import report_to_json
from .status import HealthStatus, http_status_for

HEALTH_STATUS_TOPIC = "subms.health.status"


class ProbeKind(Enum):
    LIVENESS = "LIVENESS"
    READINESS = "READINESS"
    STARTUP = "STARTUP"


class RefreshMode(Enum):
    SYNC = "SYNC"
    ASYNC = "ASYNC"


@dataclass
class RefreshPolicy:
    interval_ms: int = 30_000
    probe_kinds: List[ProbeKind] = field(default_factory=lambda: [ProbeKind.READINESS])
    critical: bool = True

    def with_interval_ms(self, ms: int) -> "RefreshPolicy":
        self.interval_ms = ms
        return self

    def with_kinds(self, kinds: List[ProbeKind]) -> "RefreshPolicy":
        self.probe_kinds = list(kinds)
        return self

    def set_critical(self, critical: bool) -> "RefreshPolicy":
        self.critical = critical
        return self

    def includes(self, kind: ProbeKind) -> bool:
        return kind in self.probe_kinds


@dataclass
class HealthConfig:
    mode: RefreshMode = RefreshMode.ASYNC
    tick_ms: int = 1_000
    dispatch: DispatchMode = DispatchMode.ASYNC
    stale_factor: float = 1.5

    @staticmethod
    def sync() -> "HealthConfig":
        return HealthConfig(mode=RefreshMode.SYNC, dispatch=DispatchMode.SYNC)


def _level_for(status: HealthStatus) -> EventLevel:
    if status == HealthStatus.DOWN:
        return EventLevel.ERROR
    if status in (HealthStatus.DEGRADED, HealthStatus.WARN):
        return EventLevel.WARN
    return EventLevel.INFO


def _code_for(status: HealthStatus, probe: Optional[ProbeKind]) -> int:
    if probe is None:
        return http_status_for(status)
    if probe == ProbeKind.LIVENESS:
        return 503 if status == HealthStatus.DOWN else 200
    if probe == ProbeKind.READINESS:
        return 503 if status in (HealthStatus.DEGRADED, HealthStatus.DOWN) else 200
    # STARTUP: not ready until UP/WARN
    return 200 if status in (HealthStatus.UP, HealthStatus.WARN) else 503


class ServerIndicator(HealthIndicator):
    def __init__(self) -> None:
        self._start = time.monotonic()
        self._pid = os.getpid()
        try:
            self._hostname = socket.gethostname()
        except Exception:
            self._hostname = "unknown"

    def name(self) -> str:
        return "server"

    def check(self) -> ComponentHealth:
        return (
            ComponentHealth.up()
            .with_detail("pid", self._pid)
            .with_detail("hostname", self._hostname)
            .with_detail("uptime_ms", int((time.monotonic() - self._start) * 1000))
        )


class HealthRegistry:
    def __init__(self, config: Optional[HealthConfig] = None, clock: Optional[Clock] = None) -> None:
        self._config = config or HealthConfig()
        self._clock = clock or SystemClock()
        self._lock = threading.Lock()
        self._indicators: List[tuple] = []  # (indicator, policy)
        self._cache: Dict[int, tuple] = {}  # index -> (component, refreshed_at_ms)
        self._snapshot: Optional[Dict[Optional[ProbeKind], tuple]] = None
        self._dispatcher = EventDispatcher(self._config.dispatch)
        self._emitter = self._dispatcher.handle()
        self._prev_overall: Optional[HealthStatus] = None
        self._prev_components: Dict[str, HealthStatus] = {}
        self._thread: Optional[threading.Thread] = None
        self._stop = threading.Event()

    # ---- construction helpers ----

    @staticmethod
    def with_system_sections(config: Optional[HealthConfig] = None) -> "HealthRegistry":
        r = HealthRegistry(config)
        r.register(
            ServerIndicator(),
            RefreshPolicy(interval_ms=5_000, probe_kinds=list(ProbeKind), critical=False),
        )
        deploy = (
            EnvSection("deploy")
            .prefix("KICKSTART_")
            .strip_prefix_in_key(True)
            .lowercase_keys(True)
            .redact_secrets()
        )
        r.register_section(
            deploy, RefreshPolicy(interval_ms=60_000, probe_kinds=list(ProbeKind), critical=False)
        )
        return r

    # ---- registration ----

    def register(self, indicator: HealthIndicator, policy: Optional[RefreshPolicy] = None) -> "HealthRegistry":
        with self._lock:
            self._indicators.append((indicator, policy or RefreshPolicy()))
        return self

    def register_fn(self, name: str, fn: Callable[[], ComponentHealth], policy: Optional[RefreshPolicy] = None) -> "HealthRegistry":
        return self.register(fn_indicator(name, fn), policy)

    def register_section(self, section: EnvSection, policy: Optional[RefreshPolicy] = None) -> "HealthRegistry":
        return self.register(section.into_indicator(), policy)

    def add_listener(self, listener: EventListener) -> "HealthRegistry":
        self._dispatcher.add_listener(listener)
        return self

    def add_bridge(self, bridge: EventBridge) -> "HealthRegistry":
        self._dispatcher.add_bridge(bridge)
        return self

    # ---- refresh ----

    def refresh_now(self) -> None:
        self._rebuild(force=True)

    def refresh_due(self) -> None:
        self._rebuild(force=False)

    def start(self) -> None:
        self._rebuild(force=True)
        if self._config.mode == RefreshMode.SYNC or self._thread is not None:
            return
        self._stop.clear()
        tick = max(1, self._config.tick_ms) / 1000.0

        def run() -> None:
            while not self._stop.wait(tick):
                self._rebuild(force=False)

        self._thread = threading.Thread(target=run, name="subms-health-refresh", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        if self._thread is not None:
            self._stop.set()
            self._thread.join()
            self._thread = None
        self._dispatcher.stop()

    # ---- render ----

    def _ensure_snapshot(self) -> None:
        if self._snapshot is None:
            self._rebuild(force=True)

    def render(self) -> tuple:
        self._ensure_snapshot()
        return self._snapshot[None]

    def render_liveness(self) -> tuple:
        self._ensure_snapshot()
        return self._snapshot[ProbeKind.LIVENESS]

    def render_readiness(self) -> tuple:
        self._ensure_snapshot()
        return self._snapshot[ProbeKind.READINESS]

    def render_startup(self) -> tuple:
        self._ensure_snapshot()
        return self._snapshot[ProbeKind.STARTUP]

    def status(self) -> HealthStatus:
        self._ensure_snapshot()
        return self._snapshot["__status__"]

    # ---- internals ----

    def _rebuild(self, force: bool) -> None:
        now = self._clock.now_ms()
        stamp = self._clock.now_rfc3339()
        with self._lock:
            indicators = list(self._indicators)
            for i, (ind, pol) in enumerate(indicators):
                cached = self._cache.get(i)
                due = force or cached is None or (now - cached[1]) >= pol.interval_ms
                if due:
                    self._cache[i] = (ind.check(), now)

            snapshot: Dict[object, tuple] = {}
            overall_status = None
            for probe in (None, ProbeKind.LIVENESS, ProbeKind.READINESS, ProbeKind.STARTUP):
                status, code, body = self._build_view(indicators, now, stamp, probe)
                snapshot[probe] = (code, body)
                if probe is None:
                    overall_status = status
            snapshot["__status__"] = overall_status

            current: Dict[str, HealthStatus] = {}
            for i, (ind, _pol) in enumerate(indicators):
                cached = self._cache.get(i)
                current[ind.name()] = (
                    cached[0].effective_status() if cached else HealthStatus.UNKNOWN
                )
            self._snapshot = snapshot

        self._emit_changes(overall_status, current, stamp)

    def _build_view(self, indicators, now, stamp, probe):
        entries = []
        contributed = []
        for i, (ind, pol) in enumerate(indicators):
            if probe is not None and not pol.includes(probe):
                continue
            cached = self._cache.get(i)
            if cached is not None:
                comp, refreshed_at = cached
            else:
                comp = ComponentHealth.unknown().with_detail("state", "pending")
                refreshed_at = now
            eff = comp.effective_status()
            if (not pol.critical) and eff in (HealthStatus.DOWN, HealthStatus.DEGRADED):
                contrib = HealthStatus.WARN
            else:
                contrib = eff
            contributed.append(contrib)
            age_ms = max(0, now - refreshed_at)
            stale = age_ms > pol.interval_ms * self._config.stale_factor
            entries.append((ind.name(), eff.value, age_ms, stale, comp))
        status = HealthStatus.aggregate(contributed)
        entries.sort(key=lambda e: e[0])
        body = report_to_json(status.value, stamp, entries)
        return status, _code_for(status, probe), body

    def _emit_changes(self, overall: HealthStatus, current: Dict[str, HealthStatus], at: str) -> None:
        changes = []
        if self._prev_overall is not None and self._prev_overall != overall:
            changes.append(("overall", self._prev_overall, overall))
        self._prev_overall = overall
        for name, st in current.items():
            old = self._prev_components.get(name)
            if old is not None and old != st:
                changes.append((name, old, st))
        self._prev_components = dict(current)
        for scope, frm, to in changes:
            event = (
                Event.builder(HEALTH_STATUS_TOPIC)
                .level(_level_for(to))
                .at(at)
                .message(f"{scope}: {frm.value} -> {to.value}")
                .attr("scope", scope)
                .attr("from", frm.value)
                .attr("to", to.value)
                .build()
            )
            self._emitter.emit(event)
