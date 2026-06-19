package com.submillisecond.recipes.eventsaga;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import com.submillisecond.recipes.events.EmitHandle;
import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventLevel;

/**
 * A saga: a named sequence of compensating steps. {@link #run} executes forwards
 * in order; the first forward failure rolls back the completed steps in reverse.
 *
 * <p>In-process orchestration only - durability, distribution, and the steps'
 * own latency are out of scope (pair with subms-ts-wal to persist the step log).
 */
public final class Saga {
    private record Step(String name, SagaAction forward, SagaAction compensate) {}

    private final String name;
    private final List<Step> steps = new ArrayList<>();
    private EmitHandle emitter;

    public Saga(String name) {
        this.name = name;
    }

    public Saga withEmitter(EmitHandle emitter) {
        this.emitter = emitter;
        return this;
    }

    public Saga step(String name, SagaAction forward, SagaAction compensate) {
        steps.add(new Step(name, forward, compensate));
        return this;
    }

    private void emit(String step, String phase, String reason) {
        if (emitter == null) {
            return;
        }
        EventLevel level = switch (phase) {
            case "forward_failed", "compensation_failed" -> EventLevel.ERROR;
            case "compensating", "compensated" -> EventLevel.WARN;
            default -> EventLevel.INFO;
        };
        var b = Event.builder("subms.saga").level(level).attr("saga", name).attr("step", step).attr("phase", phase);
        if (reason != null) {
            b = b.message(reason);
        }
        emitter.emit(b.build());
    }

    private static String msg(Exception e) {
        return e.getMessage() != null ? e.getMessage() : "";
    }

    public SagaReport run() {
        List<Integer> ran = new ArrayList<>();
        for (int i = 0; i < steps.size(); i++) {
            Step step = steps.get(i);
            emit(step.name(), "forward_started", null);
            try {
                step.forward().run();
            } catch (Exception e) {
                String reason = msg(e);
                emit(step.name(), "forward_failed", reason);
                List<String> compensated = new ArrayList<>();
                List<Map.Entry<String, String>> failures = new ArrayList<>();
                for (int k = ran.size() - 1; k >= 0; k--) {
                    Step s = steps.get(ran.get(k));
                    emit(s.name(), "compensating", null);
                    try {
                        s.compensate().run();
                        compensated.add(s.name());
                        emit(s.name(), "compensated", null);
                    } catch (Exception ce) {
                        failures.add(Map.entry(s.name(), msg(ce)));
                        emit(s.name(), "compensation_failed", msg(ce));
                    }
                }
                return new SagaReport(Outcome.COMPENSATED, step.name(), reason, ranNames(ran), compensated, failures);
            }
            ran.add(i);
            emit(step.name(), "forward_completed", null);
        }
        emit(name, "committed", null);
        return new SagaReport(Outcome.COMMITTED, null, null, ranNames(ran), List.of(), List.of());
    }

    private List<String> ranNames(List<Integer> ran) {
        List<String> out = new ArrayList<>();
        for (int idx : ran) {
            out.add(steps.get(idx).name());
        }
        return out;
    }
}
