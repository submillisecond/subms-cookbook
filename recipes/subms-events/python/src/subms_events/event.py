"""The structured Event, its EventLevel, and a fluent EventBuilder. JSON is
hand-built and deterministic - byte-equivalent to the Rust + Java ports."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, Optional


class EventLevel(Enum):
    TRACE = "TRACE"
    DEBUG = "DEBUG"
    INFO = "INFO"
    WARN = "WARN"
    ERROR = "ERROR"


def _enc(s: str) -> str:
    # json.dumps escaping matches the Rust serializer: named escapes + \u00xx.
    return json.dumps(s, ensure_ascii=False)


@dataclass
class Event:
    topic: str
    level: EventLevel = EventLevel.INFO
    at: str = ""
    message: Optional[str] = None
    attributes: Dict[str, str] = field(default_factory=dict)

    @staticmethod
    def builder(topic: str) -> "EventBuilder":
        return EventBuilder(topic)

    @staticmethod
    def transition(topic: str, level: EventLevel, scope: str, frm: str, to: str) -> "Event":
        return (
            EventBuilder(topic)
            .level(level)
            .attr("scope", scope)
            .attr("from", frm)
            .attr("to", to)
            .build()
        )

    def attr(self, key: str) -> Optional[str]:
        return self.attributes.get(key)

    def to_json(self) -> str:
        out = '{"topic":' + _enc(self.topic)
        out += ',"level":' + _enc(self.level.value)
        out += ',"at":' + _enc(self.at)
        if self.message is not None:
            out += ',"message":' + _enc(self.message)
        if self.attributes:
            inner = ",".join(f"{_enc(k)}:{_enc(self.attributes[k])}" for k in sorted(self.attributes))
            out += ',"attributes":{' + inner + "}"
        return out + "}"


class EventBuilder:
    def __init__(self, topic: str) -> None:
        self._topic = topic
        self._level = EventLevel.INFO
        self._at = ""
        self._message: Optional[str] = None
        self._attributes: Dict[str, str] = {}

    def level(self, level: EventLevel) -> "EventBuilder":
        self._level = level
        return self

    def at(self, at: str) -> "EventBuilder":
        self._at = at
        return self

    def message(self, message: str) -> "EventBuilder":
        self._message = message
        return self

    def attr(self, key: str, value: str) -> "EventBuilder":
        self._attributes[key] = value
        return self

    def build(self) -> Event:
        return Event(
            topic=self._topic,
            level=self._level,
            at=self._at,
            message=self._message,
            attributes=dict(self._attributes),
        )
