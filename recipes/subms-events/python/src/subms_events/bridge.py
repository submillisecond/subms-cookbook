"""The EventBridge sink interface + a BridgeListener adapter."""

from __future__ import annotations

from .event import Event
from .listener import EventListener


class EventBridge:
    """An external event sink. ``forward`` is called per event; ``flush`` is an
    optional hook for buffered bridges (default no-op)."""

    def name(self) -> str:  # pragma: no cover - abstract
        raise NotImplementedError

    def forward(self, event: Event) -> None:  # pragma: no cover - abstract
        raise NotImplementedError

    def flush(self) -> None:
        pass


class BridgeListener(EventListener):
    """Adapts an EventBridge into an EventListener."""

    def __init__(self, bridge: EventBridge) -> None:
        self._bridge = bridge

    def name(self) -> str:
        return self._bridge.name()

    def on_event(self, event: Event) -> None:
        self._bridge.forward(event)
