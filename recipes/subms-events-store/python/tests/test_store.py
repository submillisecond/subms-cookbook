import time

from subms_events_store import DispatchMode, Event, EventStore, Projector, listener, replay


def ev(topic):
    return Event.builder(topic).at("t").build()


def test_append_offsets():
    s = EventStore()
    assert s.is_empty()
    assert s.append(ev("a")) == 0
    assert s.append(ev("b")) == 1
    assert len(s) == 2
    assert not s.is_empty()


def test_get_and_read_from():
    s = EventStore()
    for t in ["a", "b", "c"]:
        s.append(ev(t))
    assert s.get(1).topic == "b"
    assert s.get(9) is None
    assert [e.topic for e in s.read_from(1)] == ["b", "c"]
    assert s.read_from(9) == []


def test_by_topic():
    s = EventStore()
    for t in ["x", "y", "x"]:
        s.append(ev(t))
    assert len(list(s.by_topic("x"))) == 2
    assert len(list(s.by_topic("z"))) == 0


def test_empty_json():
    assert EventStore().to_json() == "[]"


def test_replay_folds():
    s = EventStore()
    for t in ["hit", "miss", "hit"]:
        s.append(ev(t))
    hits = replay(s, 0, lambda n, e: n + 1 if e.topic == "hit" else n)
    assert hits == 2


def test_replay_empty_initial():
    assert replay(EventStore(), 42, lambda n, e: n + 1) == 42


def test_projector_incremental():
    s = EventStore()
    s.append(ev("a"))
    s.append(ev("b"))
    p = Projector(0)
    p.catch_up(s, lambda n, e: n + 1)
    assert p.state() == 2
    assert p.position() == 2
    s.append(ev("c"))
    p.catch_up(s, lambda n, e: n + 1)
    assert p.state() == 3


def test_projector_twice_noop():
    s = EventStore()
    s.append(ev("a"))
    p = Projector(0)
    p.catch_up(s, lambda n, e: n + 1)
    p.catch_up(s, lambda n, e: n + 1)
    assert p.state() == 1


def test_subscribe_sync():
    hits = []
    s = EventStore()
    s.subscribe(listener(lambda e: hits.append(1)))
    s.append(ev("a"))
    s.append(ev("b"))
    assert len(hits) == 2


def test_subscribe_async():
    seen = []
    s = EventStore(DispatchMode.ASYNC)
    s.subscribe(listener(lambda e: seen.append(e.topic)))
    s.append(ev("a"))
    s.append(ev("b"))
    for _ in range(100):
        if len(seen) >= 2:
            break
        time.sleep(0.01)
    s.stop()
    assert len(seen) == 2


def test_cross_language_store_fixture():
    s = EventStore()
    s.append(Event.builder("user.created").at("2026-06-18T00:00:00Z").attr("id", "7").build())
    s.append(Event.builder("user.renamed").at("2026-06-18T00:00:01Z").attr("id", "7").attr("name", "ko").build())
    assert s.to_json() == (
        '[{"topic":"user.created","level":"INFO","at":"2026-06-18T00:00:00Z","attributes":{"id":"7"}},'
        '{"topic":"user.renamed","level":"INFO","at":"2026-06-18T00:00:01Z","attributes":{"id":"7","name":"ko"}}]'
    )


def test_stress_append_replay():
    s = EventStore()
    for _ in range(10_000):
        s.append(ev("e"))
    assert replay(s, 0, lambda n, e: n + 1) == 10_000
    assert len(s) == 10_000
