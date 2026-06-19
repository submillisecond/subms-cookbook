"""Property-based invariant tests over randomized scenarios (seeded). The
invariant must hold for every generated input, not just hand-picked cases."""

import random

from subms_events import CompositeListener, Event, EventDispatcher, EventLevel, FilterListener, listener

LEVELS = [EventLevel.INFO, EventLevel.WARN, EventLevel.ERROR]


def test_prop_sync_delivers_full_sequence_in_order():
    rng = random.Random(1)
    for _ in range(500):
        topics = [f"t{rng.randrange(5)}" for _ in range(rng.randrange(20))]
        seen = []
        bus = EventDispatcher.sync()
        bus.add_listener(listener(lambda e: seen.append(e.topic)))
        for t in topics:
            bus.emit(Event.builder(t).build())
        assert seen == topics


def test_prop_filter_forwards_exactly_matching():
    rng = random.Random(2)
    for _ in range(500):
        target = rng.choice(LEVELS)
        evs = [rng.choice(LEVELS) for _ in range(rng.randrange(30))]
        cnt = {"n": 0}

        def inc(_e):
            cnt["n"] += 1

        f = FilterListener(lambda e: e.level == target, listener(inc))
        bus = EventDispatcher.sync()
        bus.add_listener(f)
        for lv in evs:
            bus.emit(Event.builder("x").level(lv).build())
        assert cnt["n"] == sum(1 for l in evs if l == target)


def test_prop_composite_each_child_sees_all():
    rng = random.Random(3)
    for _ in range(300):
        k = 1 + rng.randrange(5)
        emits = rng.randrange(25)
        counts = [{"n": 0} for _ in range(k)]
        ls = [listener(lambda e, c=c: c.__setitem__("n", c["n"] + 1)) for c in counts]
        bus = EventDispatcher.sync()
        bus.add_listener(CompositeListener(ls))
        for _ in range(emits):
            bus.emit(Event.builder("x").build())
        for c in counts:
            assert c["n"] == emits
