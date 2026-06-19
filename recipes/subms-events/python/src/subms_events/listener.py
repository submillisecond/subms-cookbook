"""Listeners: the EventListener protocol plus closure / composite / filter
adapters."""

from __future__ import annotations

from typing import Callable, List, Sequence

from .event import Event


class EventListener:
    """Base listener. Subclass and override on_event, or use ``listener(fn)``."""

    def on_event(self, event: Event) -> None:  # pragma: no cover - abstract
        raise NotImplementedError


class FnEventListener(EventListener):
    def __init__(self, fn: Callable[[Event], None]) -> None:
        self._fn = fn

    def on_event(self, event: Event) -> None:
        self._fn(event)


def listener(fn: Callable[[Event], None]) -> EventListener:
    """Build a listener from a callable: ``listener(lambda e: ...)``."""
    return FnEventListener(fn)


class CompositeListener(EventListener):
    """Fan-out: deliver each event to several listeners in order."""

    def __init__(self, listeners: Sequence[EventListener]) -> None:
        self._listeners: List[EventListener] = list(listeners)

    def push(self, l: EventListener) -> "CompositeListener":
        self._listeners.append(l)
        return self

    def on_event(self, event: Event) -> None:
        for l in self._listeners:
            l.on_event(event)


class FilterListener(EventListener):
    """Gate: forward to the inner listener only when the predicate passes."""

    def __init__(self, predicate: Callable[[Event], bool], inner: EventListener) -> None:
        self._predicate = predicate
        self._inner = inner

    def on_event(self, event: Event) -> None:
        if self._predicate(event):
            self._inner.on_event(event)
