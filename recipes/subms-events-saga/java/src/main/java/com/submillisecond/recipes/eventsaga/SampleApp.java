package com.submillisecond.recipes.eventsaga;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.events.EventDispatcher;

/**
 * Sample app: a tour of {@code subms-events-saga} on a trade-settlement workflow.
 * Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.eventsaga.SampleApp}
 *
 * <p>The recipe has no optional variants, so the base executor is the whole
 * sample. It tours both saga outcomes over the same four steps
 * (reserve -> match -> settle -> confirm):
 *
 * <ul>
 *   <li>commit     - every step succeeds, nothing is rolled back
 *   <li>compensate - settle fails, so match and reserve unwind in reverse
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        commitPath();
        compensatePath();
    }

    /** A shared ledger the steps mutate: forward effects and compensations, in fire order. */
    static final class Ledger {
        final List<String> applied = new ArrayList<>();
        final List<String> undone = new ArrayList<>();
    }

    static void commitPath() {
        System.out.println("== commit: settlement completes end to end ==");
        Ledger ledger = new Ledger();
        SagaReport report = buildSettlement(ledger, false).run();

        System.out.println("  outcome:  " + report.outcome().token());
        System.out.println("  applied:  " + ledger.applied);
        System.out.println("  json:     " + report.toJson());
        if (report.outcome() != Outcome.COMMITTED) throw new AssertionError("should commit");
        if (!ledger.applied.equals(List.of("reserve", "match", "settle", "confirm"))) {
            throw new AssertionError("every forward should apply");
        }
        if (!ledger.undone.isEmpty()) throw new AssertionError("a clean commit rolls nothing back");
    }

    static void compensatePath() {
        System.out.println("\n== compensate: settle fails, prior steps unwind in reverse ==");
        Ledger ledger = new Ledger();
        List<String> phases = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(e -> phases.add(e.attr("step") + ":" + e.attr("phase")));

        SagaReport report = buildSettlement(ledger, true).withEmitter(bus.handle()).run();

        System.out.println("  outcome:      " + report.outcome().token());
        System.out.println("  failed_step:  " + report.failedStep());
        System.out.println("  applied:      " + ledger.applied);
        System.out.println("  compensated:  " + report.compensated());
        System.out.println("  undone:       " + ledger.undone);
        System.out.println("  json:         " + report.toJson());

        if (report.outcome() != Outcome.COMPENSATED) throw new AssertionError("should compensate");
        if (!"settle".equals(report.failedStep())) throw new AssertionError("settle should fail");
        if (!ledger.applied.equals(List.of("reserve", "match"))) throw new AssertionError("prefix applied");
        if (!report.compensated().equals(List.of("match", "reserve"))) {
            throw new AssertionError("reverse-order rollback");
        }
        if (!ledger.undone.equals(List.of("match", "reserve"))) throw new AssertionError("undone in reverse");
        if (!phases.contains("settle:forward_failed")) throw new AssertionError("failure emitted");
        if (!phases.contains("reserve:compensated")) throw new AssertionError("compensation emitted");
        System.out.println("  events:       " + phases.size() + " lifecycle emissions");
    }

    static Saga buildSettlement(Ledger ledger, boolean settleFails) {
        Saga saga = new Saga("trade-settlement");
        for (String name : new String[] {"reserve", "match", "settle", "confirm"}) {
            saga = saga.step(name,
                    () -> {
                        if (name.equals("settle") && settleFails) {
                            throw new RuntimeException("cash leg short");
                        }
                        ledger.applied.add(name);
                    },
                    () -> ledger.undone.add(name));
        }
        return saga;
    }
}
