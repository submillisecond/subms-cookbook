"""The compensating-step executor. Steps have a forward action and a
compensation; ``run`` executes forwards in order and, on the first forward
failure (a raised exception), runs the completed steps' compensations in reverse.

In-process orchestration only - durability, distribution, and the steps' own
latency are out of scope (pair with subms-ts-wal to persist the step log)."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable, List, Optional, Tuple

from subms_events import Event, EventLevel

Action = Callable[[], None]


class Outcome(Enum):
    COMMITTED = "COMMITTED"
    COMPENSATED = "COMPENSATED"


def _enc(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def _arr(items) -> str:
    return "[" + ",".join(_enc(x) for x in items) + "]"


@dataclass
class SagaReport:
    outcome: Outcome
    failed_step: Optional[str] = None
    reason: Optional[str] = None
    forward_ran: List[str] = field(default_factory=list)
    compensated: List[str] = field(default_factory=list)
    compensation_failures: List[Tuple[str, str]] = field(default_factory=list)

    def is_committed(self) -> bool:
        return self.outcome == Outcome.COMMITTED

    def to_json(self) -> str:
        out = '{"outcome":' + _enc(self.outcome.value)
        if self.failed_step is not None:
            out += ',"failed_step":' + _enc(self.failed_step)
        if self.reason is not None:
            out += ',"reason":' + _enc(self.reason)
        out += ',"forward_ran":' + _arr(self.forward_ran)
        if self.outcome == Outcome.COMPENSATED:
            out += ',"compensated":' + _arr(self.compensated)
            pairs = ",".join("[" + _enc(s) + "," + _enc(r) + "]" for s, r in self.compensation_failures)
            out += ',"compensation_failures":[' + pairs + "]"
        return out + "}"


class _Step:
    def __init__(self, name: str, forward: Action, compensate: Action) -> None:
        self.name = name
        self.forward = forward
        self.compensate = compensate


class Saga:
    """A named sequence of compensating steps."""

    def __init__(self, name: str) -> None:
        self._name = name
        self._steps: List[_Step] = []
        self._emitter = None

    def with_emitter(self, emitter) -> "Saga":
        self._emitter = emitter
        return self

    def step(self, name: str, forward: Action, compensate: Action) -> "Saga":
        self._steps.append(_Step(name, forward, compensate))
        return self

    def _emit(self, step: str, phase: str, reason: Optional[str] = None) -> None:
        if self._emitter is None:
            return
        if phase in ("forward_failed", "compensation_failed"):
            level = EventLevel.ERROR
        elif phase in ("compensating", "compensated"):
            level = EventLevel.WARN
        else:
            level = EventLevel.INFO
        b = Event.builder("subms.saga").level(level).attr("saga", self._name).attr("step", step).attr("phase", phase)
        if reason is not None:
            b = b.message(reason)
        self._emitter.emit(b.build())

    def run(self) -> SagaReport:
        ran: List[int] = []
        for i, step in enumerate(self._steps):
            self._emit(step.name, "forward_started")
            try:
                step.forward()
            except Exception as e:  # noqa: BLE001 - the failure reason is the message
                reason = str(e)
                self._emit(step.name, "forward_failed", reason)
                compensated: List[str] = []
                failures: List[Tuple[str, str]] = []
                for j in reversed(ran):
                    s = self._steps[j]
                    self._emit(s.name, "compensating")
                    try:
                        s.compensate()
                        compensated.append(s.name)
                        self._emit(s.name, "compensated")
                    except Exception as ce:  # noqa: BLE001
                        failures.append((s.name, str(ce)))
                        self._emit(s.name, "compensation_failed", str(ce))
                return SagaReport(
                    Outcome.COMPENSATED,
                    failed_step=step.name,
                    reason=reason,
                    forward_ran=[self._steps[k].name for k in ran],
                    compensated=compensated,
                    compensation_failures=failures,
                )
            ran.append(i)
            self._emit(step.name, "forward_completed")
        self._emit(self._name, "committed")
        return SagaReport(Outcome.COMMITTED, forward_ran=[self._steps[k].name for k in ran])
