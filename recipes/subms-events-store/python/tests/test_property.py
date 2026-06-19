"""Property test: an incremental Projector must agree with a full replay at every
catch-up point, for any random interleaving of appends and catch-ups."""

import random

from subms_events_store import Event, EventStore, Projector, replay


def test_prop_incremental_projection_equals_full_replay():
    rng = random.Random(7)
    for _ in range(300):
        store = EventStore()
        proj = Projector(0)
        for _ in range(rng.randrange(40)):
            if rng.randrange(3) != 0:
                store.append(Event.builder(f"t{rng.randrange(4)}").at("t").build())
            else:
                proj.catch_up(store, lambda n, e: n + 1)
                full = replay(store, 0, lambda n, e: n + 1)
                assert proj.state() == full
                assert proj.position() == len(store)
        proj.catch_up(store, lambda n, e: n + 1)
        assert proj.state() == len(store)
        assert proj.position() == len(store)


def test_prop_offsets_dense_and_monotonic():
    rng = random.Random(13)
    for _ in range(200):
        store = EventStore()
        n = rng.randrange(50)
        for i in range(n):
            assert store.append(Event.builder("e").at("t").build()) == i
        assert len(store) == n
