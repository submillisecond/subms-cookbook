"""The append-only event log. Stores subms-events Events, returns an offset per
append, and fans appended events to live subscribers via an EventDispatcher."""

from __future__ import annotations

from typing import Iterator, List, Optional

from subms_events import DispatchMode, Event, EventDispatcher, EventListener


class EventStore:
    """In-memory, append-only log with offset addressing + live subscriptions.
    Durability is out of scope - pair with subms-ts-wal to persist."""

    def __init__(self, dispatch: DispatchMode = DispatchMode.SYNC) -> None:
        self._log: List[Event] = []
        self._dispatcher = EventDispatcher(dispatch)

    def append(self, event: Event) -> int:
        offset = len(self._log)
        self._log.append(event)
        self._dispatcher.emit(event)
        return offset

    def __len__(self) -> int:
        return len(self._log)

    def is_empty(self) -> bool:
        return not self._log

    def get(self, offset: int) -> Optional[Event]:
        if 0 <= offset < len(self._log):
            return self._log[offset]
        return None

    def events(self) -> List[Event]:
        return self._log

    def read_from(self, offset: int) -> List[Event]:
        i = min(offset, len(self._log))
        return self._log[i:]

    def by_topic(self, topic: str) -> Iterator[Event]:
        return (e for e in self._log if e.topic == topic)

    def subscribe(self, listener: EventListener) -> None:
        self._dispatcher.add_listener(listener)

    def stop(self) -> None:
        self._dispatcher.stop()

    def to_json(self) -> str:
        return "[" + ",".join(e.to_json() for e in self._log) + "]"
