"""Projections: fold the log into a read model. ``replay`` does a full fold;
``Projector`` remembers its offset so ``catch_up`` applies only the tail."""

from __future__ import annotations

from typing import Callable, Generic, TypeVar

from subms_events import Event

from .event_store import EventStore

S = TypeVar("S")


def replay(store: EventStore, initial: S, apply: Callable[[S, Event], S]) -> S:
    """Full fold over every event. ``apply`` returns the new state."""
    state = initial
    for e in store.events():
        state = apply(state, e)
    return state


class Projector(Generic[S]):
    """Incremental projection: holds state + the next offset. ``catch_up`` is the
    sub-ms path - it applies only the events appended since last time."""

    def __init__(self, initial: S) -> None:
        self._state = initial
        self._next = 0

    def state(self) -> S:
        return self._state

    def position(self) -> int:
        return self._next

    def catch_up(self, store: EventStore, apply: Callable[[S, Event], S]) -> S:
        for e in store.read_from(self._next):
            self._state = apply(self._state, e)
        self._next = len(store)
        return self._state
