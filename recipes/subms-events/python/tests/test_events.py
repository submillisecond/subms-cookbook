import threading
import time

from subms_events import (
    BridgeListener,
    CompositeListener,
    DispatchMode,
    Event,
    EventBridge,
    EventDispatcher,
    EventLevel,
    FilterListener,
    OverflowPolicy,
    listener,
)


def _gated(release, state):
    """A listener whose first event blocks on `release` so the consumer parks
    and the bounded queue fills deterministically. `state` collects topics."""

    def l(e):
        if state["first"]:
            state["first"] = False
            state["entered"] = True
            release.wait()
        state["delivered"].append(e.topic)

    return listener(l)


def _wait(cond):
    for _ in range(400):
        if cond():
            return
        time.sleep(0.005)


def test_bounded_drop_newest():
    release = threading.Event()
    state = {"first": True, "entered": False, "delivered": []}
    bus = EventDispatcher.bounded(2, OverflowPolicy.DROP_NEWEST)
    bus.add_listener(_gated(release, state))
    bus.emit(Event.builder("e1").build())
    _wait(lambda: state["entered"])
    bus.emit(Event.builder("e2").build())
    bus.emit(Event.builder("e3").build())
    bus.emit(Event.builder("e4").build())  # full -> dropped
    assert bus.dropped() == 1
    release.set()
    _wait(lambda: len(state["delivered"]) >= 3)
    bus.stop()
    assert state["delivered"] == ["e1", "e2", "e3"]


def test_bounded_drop_oldest():
    release = threading.Event()
    state = {"first": True, "entered": False, "delivered": []}
    bus = EventDispatcher.bounded(2, OverflowPolicy.DROP_OLDEST)
    bus.add_listener(_gated(release, state))
    bus.emit(Event.builder("e1").build())
    _wait(lambda: state["entered"])
    bus.emit(Event.builder("e2").build())
    bus.emit(Event.builder("e3").build())
    bus.emit(Event.builder("e4").build())  # evicts e2 -> [e3, e4]
    assert bus.dropped() == 1
    release.set()
    _wait(lambda: len(state["delivered"]) >= 3)
    bus.stop()
    assert state["delivered"] == ["e1", "e3", "e4"]


def test_stress_multi_producer_no_loss():
    n = {"c": 0}
    lock = threading.Lock()
    bus = EventDispatcher.asynchronous()

    def inc(_e):
        with lock:
            n["c"] += 1

    bus.add_listener(listener(inc))
    handle = bus.handle()
    producers, per = 4, 50_000
    total = producers * per

    def produce():
        for _ in range(per):
            handle.emit(Event.builder("x").build())

    threads = [threading.Thread(target=produce) for _ in range(producers)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    for _ in range(2000):
        if n["c"] >= total:
            break
        time.sleep(0.005)
    bus.stop()
    assert n["c"] == total  # every event delivered exactly once


def test_bounded_block_delivers():
    n = {"c": 0}
    lock = threading.Lock()
    bus = EventDispatcher.bounded(8, OverflowPolicy.BLOCK)

    def inc(_e):
        with lock:
            n["c"] += 1

    bus.add_listener(listener(inc))
    for _ in range(4):
        bus.emit(Event.builder("x").build())
    _wait(lambda: n["c"] >= 4)
    bus.stop()
    assert n["c"] == 4
    assert bus.dropped() == 0


def test_event_builder_and_accessors():
    e = (
        Event.builder("cache.evict")
        .level(EventLevel.WARN)
        .at("2026-06-18T00:00:00Z")
        .message("evicted")
        .attr("keys", "128")
        .build()
    )
    assert e.topic == "cache.evict"
    assert e.level == EventLevel.WARN
    assert e.attr("keys") == "128"
    assert e.attr("missing") is None


def test_event_level_tokens():
    assert [l.value for l in EventLevel] == ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]


def test_transition_helper():
    e = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN")
    assert e.attr("scope") == "db"
    assert e.attr("from") == "UP"
    assert e.attr("to") == "DOWN"


def test_cross_language_event_fixture():
    e = (
        Event.builder("svc.status")
        .level(EventLevel.ERROR)
        .at("2026-06-18T00:00:00Z")
        .message("db down")
        .attr("from", "UP")
        .attr("to", "DOWN")
        .build()
    )
    assert e.to_json() == (
        '{"topic":"svc.status","level":"ERROR","at":"2026-06-18T00:00:00Z",'
        '"message":"db down","attributes":{"from":"UP","to":"DOWN"}}'
    )


def test_json_omits_absent_fields():
    e = Event.builder("x").at("T").build()
    assert e.to_json() == '{"topic":"x","level":"INFO","at":"T"}'


def test_json_escaping():
    e = Event.builder("x").at("T").message('a"b\\c\nd\te').build()
    assert 'a\\"b\\\\c\\nd\\te' in e.to_json()


def test_json_sorts_multiple_attributes():
    e = Event.builder("t").at("T").attr("zeta", "1").attr("alpha", "2").attr("mid", "3").build()
    assert e.to_json() == (
        '{"topic":"t","level":"INFO","at":"T",'
        '"attributes":{"alpha":"2","mid":"3","zeta":"1"}}'
    )


def test_sync_dispatch_inline():
    hits = []
    bus = EventDispatcher.sync()
    bus.add_listener(listener(lambda e: hits.append(e.topic)))
    bus.emit(Event.builder("a").build())
    bus.emit(Event.builder("b").build())
    assert hits == ["a", "b"]  # inline, no waiting


def test_async_dispatch_off_thread_and_order():
    seen = []
    bus = EventDispatcher.asynchronous()
    bus.add_listener(listener(lambda e: seen.append(e.topic)))
    for t in ["a", "b", "c", "d"]:
        bus.emit(Event.builder(t).build())
    for _ in range(200):
        if len(seen) >= 4:
            break
        time.sleep(0.005)
    bus.stop()
    assert seen == ["a", "b", "c", "d"]  # queue is FIFO


def test_async_stress_delivers_every_event():
    n = {"c": 0}
    lock = threading.Lock()

    def inc(_e):
        with lock:
            n["c"] += 1

    bus = EventDispatcher.asynchronous()
    bus.add_listener(listener(inc))
    for _ in range(5000):
        bus.emit(Event.builder("x").build())
    for _ in range(400):
        if n["c"] >= 5000:
            break
        time.sleep(0.005)
    bus.stop()
    assert n["c"] == 5000


def test_composite_fan_out():
    a, b = [], []
    composite = CompositeListener([listener(lambda e: a.append(1)), listener(lambda e: b.append(1))])
    bus = EventDispatcher.sync()
    bus.add_listener(composite)
    bus.emit(Event.builder("x").build())
    assert len(a) == 1 and len(b) == 1


def test_filter_gate():
    hits = []
    inner = listener(lambda e: hits.append(1))
    gated = FilterListener(lambda e: e.level == EventLevel.ERROR, inner)
    bus = EventDispatcher.sync()
    bus.add_listener(gated)
    bus.emit(Event.builder("a").level(EventLevel.INFO).build())
    bus.emit(Event.builder("b").level(EventLevel.ERROR).build())
    assert hits == [1]


def test_nested_filter_over_composite():
    a, b = [], []
    composite = CompositeListener([listener(lambda e: a.append(1)), listener(lambda e: b.append(1))])
    gated = FilterListener(lambda e: e.level == EventLevel.ERROR, composite)
    bus = EventDispatcher.sync()
    bus.add_listener(gated)
    bus.emit(Event.builder("ok").level(EventLevel.INFO).build())
    bus.emit(Event.builder("bad").level(EventLevel.ERROR).build())
    assert len(a) == 1 and len(b) == 1


def test_filter_dropping_all():
    hits = []
    gated = FilterListener(lambda e: False, listener(lambda e: hits.append(1)))
    bus = EventDispatcher.sync()
    bus.add_listener(gated)
    for _ in range(10):
        bus.emit(Event.builder("x").level(EventLevel.ERROR).build())
    assert hits == []


def test_bridge_receives_events():
    class CountingBridge(EventBridge):
        def __init__(self):
            self.n = 0

        def name(self):
            return "counting"

        def forward(self, event):
            self.n += 1

    bridge = CountingBridge()
    bus = EventDispatcher.sync()
    bus.add_bridge(bridge)
    bus.emit(Event.builder("x").build())
    bus.emit(Event.builder("y").build())
    assert bridge.n == 2


def test_bridge_listener_name_and_flush():
    class NamedBridge(EventBridge):
        def name(self):
            return "named"

        def forward(self, event):
            pass

    b = NamedBridge()
    b.flush()  # default no-op
    assert BridgeListener(b).name() == "named"


def test_emit_handle_from_producer():
    hits = []
    bus = EventDispatcher.sync()
    bus.add_listener(listener(lambda e: hits.append(1)))
    h = bus.handle()
    h.emit(Event.builder("a").build())
    h.emit(Event.builder("b").build())
    assert len(hits) == 2
    assert h.mode() == DispatchMode.SYNC


def test_mode_accessor():
    assert EventDispatcher.sync().mode() == DispatchMode.SYNC
    assert EventDispatcher.asynchronous().mode() == DispatchMode.ASYNC


def test_no_listener_emit_is_noop():
    bus = EventDispatcher.asynchronous()
    bus.emit(Event.builder("x").build())
    assert bus.listener_count() == 0
    bus.stop()


def test_stop_is_idempotent_and_emit_after_stop_safe():
    bus = EventDispatcher.asynchronous()
    bus.add_listener(listener(lambda e: None))
    bus.emit(Event.builder("x").build())
    bus.stop()
    bus.stop()
    bus.emit(Event.builder("y").build())  # dropped, no error


def test_sync_emit_before_listener_not_seen():
    hits = []
    bus = EventDispatcher.sync()
    bus.emit(Event.builder("early").build())
    bus.add_listener(listener(lambda e: hits.append(1)))
    bus.emit(Event.builder("late").build())
    assert hits == [1]


def test_listener_count():
    bus = EventDispatcher.sync()
    assert bus.listener_count() == 0
    bus.add_listener(listener(lambda e: None))
    bus.add_listener(listener(lambda e: None))
    assert bus.listener_count() == 2


def test_event_clone_equality():
    import copy

    e = Event.transition("svc", EventLevel.WARN, "x", "UP", "WARN")
    assert e == copy.deepcopy(e)


def test_builder_last_write_wins():
    e = (
        Event.builder("t")
        .level(EventLevel.INFO)
        .level(EventLevel.ERROR)
        .message("first")
        .message("second")
        .attr("k", "1")
        .attr("k", "2")
        .build()
    )
    assert e.level == EventLevel.ERROR
    assert e.message == "second"
    assert e.attr("k") == "2"


def test_multiple_async_listeners_all_receive():
    a, b = [], []
    lock = threading.Lock()
    bus = EventDispatcher.asynchronous()
    bus.add_listener(listener(lambda e: (lock.acquire(), a.append(1), lock.release())))
    bus.add_listener(listener(lambda e: (lock.acquire(), b.append(1), lock.release())))
    bus.emit(Event.builder("x").build())
    for _ in range(200):
        if a and b:
            break
        time.sleep(0.005)
    bus.stop()
    assert len(a) == 1 and len(b) == 1
