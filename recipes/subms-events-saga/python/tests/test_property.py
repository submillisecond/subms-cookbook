"""Property tests for the saga compensation invariant + a JSON fuzz of the report,
over randomized step counts / failure points / reasons."""

import json
import random

from subms_events_saga import Outcome, Saga

CHARS = list('aZ0 "\\\n\r\t\x08\x01/{}:') + ["é", "漢", "🦀"]


def _rand_str(rng):
    return "".join(rng.choice(CHARS) for _ in range(rng.randrange(14)))


def ok():
    return None


def _make_step(should_fail, reason):
    def f():
        if should_fail:
            raise Exception(reason)

    return f


def test_prop_compensation_invariant():
    rng = random.Random(11)
    for _ in range(1000):
        n = 1 + rng.randrange(8)
        fail = rng.randrange(n) if rng.random() < 0.5 else None
        s = Saga("p")
        for i in range(n):
            s.step(f"s{i}", _make_step(fail == i, "x"), ok)
        r = s.run()
        if fail is None:
            assert r.outcome == Outcome.COMMITTED
            assert r.forward_ran == [f"s{i}" for i in range(n)]
            assert r.compensated == []
        else:
            assert r.outcome == Outcome.COMPENSATED
            assert r.failed_step == f"s{fail}"
            ran = [f"s{i}" for i in range(fail)]
            assert r.forward_ran == ran
            assert r.compensated == list(reversed(ran))


def test_fuzz_saga_report_json_is_valid_and_roundtrips():
    rng = random.Random(0x5A6A)
    for _ in range(2000):
        n = 1 + rng.randrange(6)
        fail = rng.random() < 0.5
        fail_at = rng.randrange(n)
        reason = _rand_str(rng)
        s = Saga("p")
        for i in range(n):
            s.step(f"s{i}", _make_step(fail and i == fail_at, reason), ok)
        report = s.run()
        parsed = json.loads(report.to_json())  # must be valid JSON
        assert parsed["outcome"] == report.outcome.value
        assert isinstance(parsed["forward_ran"], list)
        if report.outcome == Outcome.COMPENSATED:
            assert parsed["reason"] == reason
            assert parsed["failed_step"] == report.failed_step
            assert isinstance(parsed["compensated"], list)
