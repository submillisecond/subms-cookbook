package com.submillisecond.recipes.eventsaga;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

import com.submillisecond.recipes.events.EventDispatcher;
import org.junit.jupiter.api.Test;

class SagaTest {
    private static final SagaAction OK = () -> {};

    private static SagaAction fail(String reason) {
        return () -> {
            throw new RuntimeException(reason);
        };
    }

    @Test
    void commitAllSucceed() {
        SagaReport r = new Saga("x").step("a", OK, OK).step("b", OK, OK).run();
        assertEquals(Outcome.COMMITTED, r.outcome());
        assertTrue(r.isCommitted());
        assertEquals(List.of("a", "b"), r.forwardRan());
        assertTrue(r.compensated().isEmpty());
        assertNull(r.failedStep());
    }

    @Test
    void compensatesInReverse() {
        List<String> order = new ArrayList<>();
        SagaReport r = new Saga("x")
                .step("a", OK, () -> order.add("a"))
                .step("b", OK, () -> order.add("b"))
                .step("c", fail("boom"), OK)
                .run();
        assertEquals(Outcome.COMPENSATED, r.outcome());
        assertEquals("c", r.failedStep());
        assertEquals("boom", r.reason());
        assertEquals(List.of("a", "b"), r.forwardRan());
        assertEquals(List.of("b", "a"), r.compensated());
        assertEquals(List.of("b", "a"), order);
    }

    @Test
    void firstStepFailureCompensatesNothing() {
        SagaReport r = new Saga("x").step("a", fail("no"), OK).step("b", OK, OK).run();
        assertEquals(Outcome.COMPENSATED, r.outcome());
        assertEquals("a", r.failedStep());
        assertTrue(r.forwardRan().isEmpty());
        assertTrue(r.compensated().isEmpty());
    }

    @Test
    void middleStepFailure() {
        SagaReport r = new Saga("x").step("a", OK, OK).step("b", fail("no"), OK).step("c", OK, OK).run();
        assertEquals(List.of("a"), r.forwardRan());
        assertEquals(List.of("a"), r.compensated());
        assertEquals("b", r.failedStep());
    }

    @Test
    void compensationFailureRecorded() {
        SagaReport r = new Saga("x").step("a", OK, fail("rollback failed")).step("b", fail("boom"), OK).run();
        assertTrue(r.compensated().isEmpty());
        assertEquals(1, r.compensationFailures().size());
        assertEquals("a", r.compensationFailures().get(0).getKey());
        assertEquals("rollback failed", r.compensationFailures().get(0).getValue());
        assertTrue(r.toJson().contains("\"compensation_failures\":[[\"a\",\"rollback failed\"]]"));
    }

    @Test
    void emptySagaCommits() {
        SagaReport r = new Saga("x").run();
        assertEquals(Outcome.COMMITTED, r.outcome());
        assertEquals("{\"outcome\":\"COMMITTED\",\"forward_ran\":[]}", r.toJson());
    }

    @Test
    void forwardActionsRun() {
        AtomicInteger hits = new AtomicInteger();
        new Saga("x").step("a", hits::incrementAndGet, OK).step("b", hits::incrementAndGet, OK).run();
        assertEquals(2, hits.get());
    }

    @Test
    void outcomeTokens() {
        assertEquals("COMMITTED", Outcome.COMMITTED.token());
        assertEquals("COMPENSATED", Outcome.COMPENSATED.token());
    }

    @Test
    void emitsLifecycleEvents() {
        List<String> phases = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(e -> phases.add(e.attr("step") + ":" + e.attr("phase")));
        new Saga("x").withEmitter(bus.handle()).step("a", OK, OK).step("b", fail("boom"), OK).run();
        assertTrue(phases.contains("a:forward_completed"));
        assertTrue(phases.contains("b:forward_failed"));
        assertTrue(phases.contains("a:compensated"));
    }

    @Test
    void emitsCommittedEvent() {
        List<String> phases = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(e -> phases.add(e.attr("phase")));
        new Saga("x").withEmitter(bus.handle()).step("a", OK, OK).run();
        assertTrue(phases.contains("committed"));
    }

    @Test
    void crossLanguageCommittedFixture() {
        SagaReport r = new Saga("x").step("a", OK, OK).step("b", OK, OK).run();
        assertEquals("{\"outcome\":\"COMMITTED\",\"forward_ran\":[\"a\",\"b\"]}", r.toJson());
    }

    @Test
    void crossLanguageCompensatedFixture() {
        SagaReport r = new Saga("x").step("a", OK, OK).step("b", OK, OK).step("c", fail("boom"), OK).run();
        assertEquals(
                "{\"outcome\":\"COMPENSATED\",\"failed_step\":\"c\",\"reason\":\"boom\","
                        + "\"forward_ran\":[\"a\",\"b\"],\"compensated\":[\"b\",\"a\"],\"compensation_failures\":[]}",
                r.toJson());
    }

    @Test
    void jsonEscapingInReason() {
        SagaReport r = new Saga("x").step("a", fail("a\"b\\c"), OK).run();
        assertTrue(r.toJson().contains("\\\"b\\\\c"));
        assertFalse(r.isCommitted());
    }
}
