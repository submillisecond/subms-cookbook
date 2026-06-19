package com.submillisecond.recipes.eventsaga;

import java.util.List;
import java.util.Map;

/** What happened when a saga ran. Deterministic JSON, byte-equivalent across ports. */
public final class SagaReport {
    private final Outcome outcome;
    private final String failedStep; // nullable
    private final String reason; // nullable
    private final List<String> forwardRan;
    private final List<String> compensated;
    private final List<Map.Entry<String, String>> compensationFailures;

    SagaReport(Outcome outcome, String failedStep, String reason, List<String> forwardRan,
            List<String> compensated, List<Map.Entry<String, String>> compensationFailures) {
        this.outcome = outcome;
        this.failedStep = failedStep;
        this.reason = reason;
        this.forwardRan = forwardRan;
        this.compensated = compensated;
        this.compensationFailures = compensationFailures;
    }

    public Outcome outcome() {
        return outcome;
    }

    public boolean isCommitted() {
        return outcome == Outcome.COMMITTED;
    }

    public String failedStep() {
        return failedStep;
    }

    public String reason() {
        return reason;
    }

    public List<String> forwardRan() {
        return forwardRan;
    }

    public List<String> compensated() {
        return compensated;
    }

    public List<Map.Entry<String, String>> compensationFailures() {
        return compensationFailures;
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"outcome\":");
        Json.str(sb, outcome.token());
        if (failedStep != null) {
            sb.append(",\"failed_step\":");
            Json.str(sb, failedStep);
        }
        if (reason != null) {
            sb.append(",\"reason\":");
            Json.str(sb, reason);
        }
        sb.append(",\"forward_ran\":");
        Json.arr(sb, forwardRan);
        if (outcome == Outcome.COMPENSATED) {
            sb.append(",\"compensated\":");
            Json.arr(sb, compensated);
            sb.append(",\"compensation_failures\":[");
            for (int i = 0; i < compensationFailures.size(); i++) {
                if (i > 0) {
                    sb.append(',');
                }
                Map.Entry<String, String> e = compensationFailures.get(i);
                sb.append('[');
                Json.str(sb, e.getKey());
                sb.append(',');
                Json.str(sb, e.getValue());
                sb.append(']');
            }
            sb.append(']');
        }
        return sb.append('}').toString();
    }
}
