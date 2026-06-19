from subms_events_saga import EventDispatcher, Outcome, Saga, listener


def ok():
    return None


def fail(reason="boom"):
    def f():
        raise Exception(reason)

    return f


def test_commit_all_succeed():
    r = Saga("x").step("a", ok, ok).step("b", ok, ok).run()
    assert r.outcome == Outcome.COMMITTED
    assert r.is_committed()
    assert r.forward_ran == ["a", "b"]
    assert r.compensated == []
    assert r.failed_step is None


def test_compensates_in_reverse():
    order = []
    r = (
        Saga("x")
        .step("a", ok, lambda: order.append("a"))
        .step("b", ok, lambda: order.append("b"))
        .step("c", fail(), ok)
        .run()
    )
    assert r.outcome == Outcome.COMPENSATED
    assert r.failed_step == "c"
    assert r.reason == "boom"
    assert r.forward_ran == ["a", "b"]
    assert r.compensated == ["b", "a"]
    assert order == ["b", "a"]


def test_first_step_failure_compensates_nothing():
    r = Saga("x").step("a", fail("no"), ok).step("b", ok, ok).run()
    assert r.outcome == Outcome.COMPENSATED
    assert r.failed_step == "a"
    assert r.forward_ran == []
    assert r.compensated == []


def test_middle_step_failure():
    r = Saga("x").step("a", ok, ok).step("b", fail("no"), ok).step("c", ok, ok).run()
    assert r.forward_ran == ["a"]
    assert r.compensated == ["a"]
    assert r.failed_step == "b"


def test_compensation_failure_recorded():
    r = Saga("x").step("a", ok, fail("rollback failed")).step("b", fail(), ok).run()
    assert r.compensated == []
    assert r.compensation_failures == [("a", "rollback failed")]


def test_empty_saga_commits():
    r = Saga("x").run()
    assert r.outcome == Outcome.COMMITTED
    assert r.to_json() == '{"outcome":"COMMITTED","forward_ran":[]}'


def test_forward_actions_run():
    hits = {"n": 0}

    def inc():
        hits["n"] += 1

    Saga("x").step("a", inc, ok).step("b", inc, ok).run()
    assert hits["n"] == 2


def test_outcome_tokens():
    assert Outcome.COMMITTED.value == "COMMITTED"
    assert Outcome.COMPENSATED.value == "COMPENSATED"


def test_emits_lifecycle_events():
    phases = []
    bus = EventDispatcher.sync()
    bus.add_listener(listener(lambda e: phases.append(f'{e.attr("step")}:{e.attr("phase")}')))
    Saga("x").with_emitter(bus.handle()).step("a", ok, ok).step("b", fail(), ok).run()
    assert "a:forward_completed" in phases
    assert "b:forward_failed" in phases
    assert "a:compensated" in phases


def test_cross_language_committed_fixture():
    r = Saga("x").step("a", ok, ok).step("b", ok, ok).run()
    assert r.to_json() == '{"outcome":"COMMITTED","forward_ran":["a","b"]}'


def test_cross_language_compensated_fixture():
    r = Saga("x").step("a", ok, ok).step("b", ok, ok).step("c", fail(), ok).run()
    assert r.to_json() == (
        '{"outcome":"COMPENSATED","failed_step":"c","reason":"boom",'
        '"forward_ran":["a","b"],"compensated":["b","a"],"compensation_failures":[]}'
    )


def test_json_escaping_in_reason():
    r = Saga("x").step("a", fail('a"b\\c'), ok).run()
    assert '\\"b\\\\c' in r.to_json()
