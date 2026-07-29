package com.submillisecond.recipes.eventsaga;

import java.util.List;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        SagaReport report = new Saga("checkout")
                .step("reserve", () -> {}, () -> {})
                .step("charge", () -> { throw new RuntimeException("card declined"); }, () -> {})
                .run();

        assertEquals(Outcome.COMPENSATED, report.outcome()); // charge failed
        assertEquals(List.of("reserve"), report.compensated()); // reserve was rolled back
        // quickstart:end
    }

    @Test
    void commitScenarioAppliesEveryStep() {
        SampleApp.Ledger ledger = new SampleApp.Ledger();
        SagaReport report = SampleApp.buildSettlement(ledger, false).run();

        assertEquals(Outcome.COMMITTED, report.outcome());
        assertEquals(List.of("reserve", "match", "settle", "confirm"), ledger.applied);
        assertTrue(ledger.undone.isEmpty(), "a clean commit rolls nothing back");
    }

    @Test
    void compensateScenarioUnwindsPrefixInReverse() {
        SampleApp.Ledger ledger = new SampleApp.Ledger();
        SagaReport report = SampleApp.buildSettlement(ledger, true).run();

        assertEquals(Outcome.COMPENSATED, report.outcome());
        assertEquals("settle", report.failedStep());
        assertEquals(List.of("reserve", "match"), ledger.applied);
        assertEquals(List.of("match", "reserve"), report.compensated());
        assertEquals(List.of("match", "reserve"), ledger.undone);
    }
}
