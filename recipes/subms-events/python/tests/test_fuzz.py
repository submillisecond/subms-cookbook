"""Fuzz the hand-rolled Event.to_json against the stdlib json parser. For
thousands of random events (quotes, backslashes, control chars, unicode), the
output must parse and round-trip every field."""

import json
import random

from subms_events import Event, EventLevel

CHARS = list('aZ0 "\\\n\r\t\x08\x0c\x01/{}:,') + ["é", "漢", "🦀"]


def rand_str(rng):
    return "".join(rng.choice(CHARS) for _ in range(rng.randrange(12)))


def test_fuzz_event_json_is_valid_and_roundtrips():
    rng = random.Random(0xC0FFEE)
    levels = list(EventLevel)
    for _ in range(3000):
        topic = rand_str(rng)
        at = rand_str(rng)
        level = rng.choice(levels)
        b = Event.builder(topic).level(level).at(at)
        has_msg = rng.random() < 0.5
        msg = rand_str(rng)
        if has_msg:
            b = b.message(msg)
        attrs = {}
        for _ in range(rng.randrange(4)):
            k = rand_str(rng)
            v = rand_str(rng)
            attrs[k] = v
            b = b.attr(k, v)
        parsed = json.loads(b.build().to_json())  # must be valid JSON
        assert parsed["topic"] == topic
        assert parsed["level"] == level.value
        assert parsed["at"] == at
        if has_msg:
            assert parsed["message"] == msg
        else:
            assert "message" not in parsed
        for k, v in attrs.items():
            assert parsed["attributes"][k] == v
