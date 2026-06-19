package com.submillisecond.recipes.eventsaga;

import java.util.ArrayList;
import java.util.List;

/** Stdout demo: a checkout saga whose charge fails, rolling back the reservation. */
public final class Demo {
    public static void main(String[] args) {
        List<String> undone = new ArrayList<>();
        SagaReport report = new Saga("checkout")
                .step("reserve_stock", () -> {}, () -> undone.add("reserve_stock"))
                .step("charge_card", () -> {
                    throw new RuntimeException("card declined");
                }, () -> {})
                .run();

        System.out.println("outcome: " + report.outcome().token());
        System.out.println("report: " + report.toJson());
        System.out.println("compensated: " + undone);
    }
}
