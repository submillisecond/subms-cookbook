"""The dispatcher. Sync: listeners run inline on the emitter. Async: events go to
a daemon thread over a queue, so a slow listener never blocks the emitter. The
async queue is unbounded by default; use ``bounded`` to cap it + pick a policy."""

from __future__ import annotations

import queue
import threading
from enum import Enum
from typing import List, Optional

from .bridge import BridgeListener, EventBridge
from .event import Event
from .listener import EventListener


class DispatchMode(Enum):
    SYNC = "SYNC"
    ASYNC = "ASYNC"


class OverflowPolicy(Enum):
    BLOCK = "BLOCK"
    DROP_NEWEST = "DROP_NEWEST"
    DROP_OLDEST = "DROP_OLDEST"


_STOP = object()


class _Inner:
    def __init__(self, mode: "DispatchMode", capacity: Optional[int], policy: "OverflowPolicy") -> None:
        self.mode = mode
        self.capacity = capacity
        self.policy = policy
        self.lock = threading.Lock()
        self.listeners: List[EventListener] = []
        self.queue: "Optional[queue.Queue]" = None
        self.dropped = 0

    def _drop(self) -> None:
        with self.lock:
            self.dropped += 1

    def dispatch(self, event: Event) -> None:
        if self.mode == DispatchMode.SYNC:
            with self.lock:
                listeners = list(self.listeners)
            for l in listeners:
                l.on_event(event)
            return
        q = self.queue
        if q is None:
            return
        if self.capacity is None:
            q.put(event)
        elif self.policy == OverflowPolicy.BLOCK:
            q.put(event)
        elif self.policy == OverflowPolicy.DROP_NEWEST:
            try:
                q.put_nowait(event)
            except queue.Full:
                self._drop()
        else:  # DROP_OLDEST
            try:
                q.put_nowait(event)
            except queue.Full:
                try:
                    q.get_nowait()
                    self._drop()
                except queue.Empty:
                    pass
                try:
                    q.put_nowait(event)
                except queue.Full:
                    self._drop()


class EmitHandle:
    """A cheap, shareable emitter for producers that don't own the dispatcher."""

    def __init__(self, inner: _Inner) -> None:
        self._inner = inner

    def emit(self, event: Event) -> None:
        self._inner.dispatch(event)

    def mode(self) -> DispatchMode:
        return self._inner.mode


class EventDispatcher:
    def __init__(self, mode: DispatchMode, capacity: Optional[int] = None,
                 policy: OverflowPolicy = OverflowPolicy.BLOCK) -> None:
        self._inner = _Inner(mode, capacity, policy)
        self._thread: Optional[threading.Thread] = None

    @staticmethod
    def sync() -> "EventDispatcher":
        return EventDispatcher(DispatchMode.SYNC)

    @staticmethod
    def asynchronous() -> "EventDispatcher":
        return EventDispatcher(DispatchMode.ASYNC)

    @staticmethod
    def bounded(capacity: int, policy: OverflowPolicy) -> "EventDispatcher":
        return EventDispatcher(DispatchMode.ASYNC, max(1, capacity), policy)

    def mode(self) -> DispatchMode:
        return self._inner.mode

    def dropped(self) -> int:
        return self._inner.dropped

    def listener_count(self) -> int:
        with self._inner.lock:
            return len(self._inner.listeners)

    def add_listener(self, l: EventListener) -> "EventDispatcher":
        with self._inner.lock:
            self._inner.listeners.append(l)
        if self._inner.mode == DispatchMode.ASYNC:
            self._ensure_thread()
        return self

    def add_bridge(self, bridge: EventBridge) -> "EventDispatcher":
        return self.add_listener(BridgeListener(bridge))

    def _ensure_thread(self) -> None:
        if self._thread is not None:
            return
        q: "queue.Queue" = queue.Queue(maxsize=self._inner.capacity or 0)
        self._inner.queue = q
        inner = self._inner

        def run() -> None:
            while True:
                item = q.get()
                if item is _STOP:
                    break
                with inner.lock:
                    listeners = list(inner.listeners)
                for l in listeners:
                    l.on_event(item)

        t = threading.Thread(target=run, name="subms-events-dispatch", daemon=True)
        t.start()
        self._thread = t

    def emit(self, event: Event) -> None:
        self._inner.dispatch(event)

    def handle(self) -> EmitHandle:
        return EmitHandle(self._inner)

    def stop(self) -> None:
        if self._thread is not None and self._inner.queue is not None:
            self._inner.queue.put(_STOP)
            self._thread.join()
            self._thread = None
            self._inner.queue = None
