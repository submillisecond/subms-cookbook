package com.submillisecond.recipes.events;

import java.util.EnumSet;

/** Stdout demo: a sync dispatcher with a level filter. */
public final class Demo {
    public static void main(String[] args) {
        EventDispatcher bus = EventDispatcher.sync();
        EnumSet<EventLevel> warnAndUp = EnumSet.of(EventLevel.WARN, EventLevel.ERROR);
        bus.addListener(new FilterListener(
                e -> warnAndUp.contains(e.level()),
                e -> System.out.println("[" + e.level().token() + "] " + e.topic() + " " + e.toJson())));

        bus.emit(Event.builder("cache.evict").level(EventLevel.INFO).attr("keys", "128").build());
        bus.emit(Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN"));
        bus.emit(Event.transition("svc.status", EventLevel.INFO, "db", "DOWN", "UP"));
        System.out.println("(info + recovery events were filtered out)");
    }
}
