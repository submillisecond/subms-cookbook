package com.submillisecond.recipes.events;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class EventTest {

    @Test
    void builderAndAccessors() {
        Event e = Event.builder("cache.evict")
                .level(EventLevel.WARN)
                .at("2026-06-18T00:00:00Z")
                .message("evicted")
                .attr("keys", "128")
                .build();
        assertEquals("cache.evict", e.topic());
        assertEquals(EventLevel.WARN, e.level());
        assertEquals("128", e.attr("keys"));
        assertNull(e.attr("missing"));
    }

    @Test
    void levelTokens() {
        assertEquals("TRACE", EventLevel.TRACE.token());
        assertEquals("INFO", EventLevel.INFO.token());
        assertEquals("ERROR", EventLevel.ERROR.token());
    }

    @Test
    void transitionHelper() {
        Event e = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN");
        assertEquals("db", e.attr("scope"));
        assertEquals("UP", e.attr("from"));
        assertEquals("DOWN", e.attr("to"));
    }

    @Test
    void crossLanguageEventFixture() {
        Event e = Event.builder("svc.status")
                .level(EventLevel.ERROR)
                .at("2026-06-18T00:00:00Z")
                .message("db down")
                .attr("from", "UP")
                .attr("to", "DOWN")
                .build();
        assertEquals(
                "{\"topic\":\"svc.status\",\"level\":\"ERROR\",\"at\":\"2026-06-18T00:00:00Z\","
                        + "\"message\":\"db down\",\"attributes\":{\"from\":\"UP\",\"to\":\"DOWN\"}}",
                e.toJson());
    }

    @Test
    void jsonOmitsAbsentFields() {
        Event e = Event.builder("x").at("T").build();
        assertEquals("{\"topic\":\"x\",\"level\":\"INFO\",\"at\":\"T\"}", e.toJson());
    }

    @Test
    void jsonEscaping() {
        Event e = Event.builder("x").at("T").message("a\"b\\c\nd\te").build();
        assertTrue(e.toJson().contains("a\\\"b\\\\c\\nd\\te"));
    }

    @Test
    void jsonSortsMultipleAttributes() {
        Event e = Event.builder("t").at("T").attr("zeta", "1").attr("alpha", "2").attr("mid", "3").build();
        assertEquals(
                "{\"topic\":\"t\",\"level\":\"INFO\",\"at\":\"T\","
                        + "\"attributes\":{\"alpha\":\"2\",\"mid\":\"3\",\"zeta\":\"1\"}}",
                e.toJson());
    }

    @Test
    void cloneEquality() {
        Event a = Event.transition("svc", EventLevel.WARN, "x", "UP", "WARN");
        Event b = Event.transition("svc", EventLevel.WARN, "x", "UP", "WARN");
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
    }

    @Test
    void builderLastWriteWins() {
        Event e = Event.builder("t")
                .level(EventLevel.INFO)
                .level(EventLevel.ERROR)
                .message("first")
                .message("second")
                .attr("k", "1")
                .attr("k", "2")
                .build();
        assertEquals(EventLevel.ERROR, e.level());
        assertEquals("second", e.message());
        assertEquals("2", e.attr("k"));
    }
}
