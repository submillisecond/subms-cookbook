package com.submillisecond.recipes.eventstore;

import com.submillisecond.recipes.events.Event;

/** Stdout demo: append events, fold a read model, print the JSON log. */
public final class Demo {
    public static void main(String[] args) {
        EventStore store = new EventStore();
        store.append(Event.builder("user.created").at("t0").attr("id", "7").build());
        store.append(Event.builder("user.renamed").at("t1").attr("id", "7").attr("name", "ko").build());
        store.append(Event.builder("user.created").at("t2").attr("id", "8").build());

        long created = Projector.replay(store, 0L, (n, e) -> e.topic().equals("user.created") ? n + 1 : n);
        System.out.println("users created: " + created);
        System.out.println("log: " + store.toJson());
    }
}
