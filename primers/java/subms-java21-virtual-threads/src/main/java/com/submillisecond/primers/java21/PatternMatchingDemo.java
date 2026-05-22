package com.submillisecond.primers.java21;

import java.util.List;
import java.util.SequencedCollection;
import java.util.LinkedHashMap;

/**
 * Java 21 record patterns + switch patterns + sequenced collections, on a
 * tiny order-event ADT.
 *
 * The shape of the language feature is what matters here, not the
 * domain. With record patterns + pattern-matched switch you destructure
 * straight into the branches: no instanceof checks, no casts, no nested
 * accessors. The compiler enforces exhaustiveness against the sealed
 * hierarchy.
 *
 * Sequenced collections give first/last/reversed without writing the
 * iteration yourself.
 */
public final class PatternMatchingDemo {
    private PatternMatchingDemo() {}

    sealed interface OrderEvent permits New, Fill, Cancel, Reject {}
    record New(long id, String symbol, long qty, long pxTicks)        implements OrderEvent {}
    record Fill(long id, long qty, long pxTicks)                       implements OrderEvent {}
    record Cancel(long id, String reason)                              implements OrderEvent {}
    record Reject(long id, String reason)                              implements OrderEvent {}

    /**
     * Switch on the event with record patterns. Each case binds the
     * fields directly. The compiler refuses to compile if a case is
     * missing - try commenting one out and see.
     */
    public static String describe(OrderEvent e) {
        return switch (e) {
            case New(var id, var symbol, var qty, var pxTicks) ->
                    "new  #" + id + "  " + qty + " " + symbol + " @ " + pxTicks;
            case Fill(var id, var qty, var pxTicks) ->
                    "fill #" + id + "  " + qty + " @ " + pxTicks;
            case Cancel(var id, var reason) ->
                    "cxl  #" + id + "  (" + reason + ")";
            case Reject(var id, var reason) ->
                    "rej  #" + id + "  (" + reason + ")";
        };
    }

    /**
     * Pattern matching with a guard - only flags fills bigger than a
     * threshold, no else-if ladder.
     */
    public static String classify(OrderEvent e) {
        return switch (e) {
            case Fill f when f.qty() >= 1_000  -> "block-trade";
            case Fill f                         -> "ordinary-fill";
            case Cancel c when c.reason().contains("timeout") -> "stale-cancel";
            case Cancel c                       -> "user-cancel";
            case New n                          -> "new-order";
            case Reject r                       -> "reject";
        };
    }

    public static void main(String[] args) {
        List<OrderEvent> tape = List.of(
                new New(1, "AAPL", 100, 19_500),
                new Fill(1, 100, 19_502),
                new New(2, "MSFT", 5_000, 41_000),
                new Fill(2, 5_000, 41_001),
                new Cancel(3, "user timeout"),
                new Reject(4, "risk")
        );

        // SequencedCollection: List<>.reversed() without copying.
        SequencedCollection<OrderEvent> latestFirst = tape.reversed();
        System.out.println("tape (newest first):");
        latestFirst.forEach(e -> System.out.println("  " + describe(e)));

        // LinkedHashMap.firstEntry() / lastEntry() / reversed() likewise.
        LinkedHashMap<Long, String> classifications = new LinkedHashMap<>();
        for (OrderEvent e : tape) {
            long id = switch (e) {
                case New n     -> n.id();
                case Fill f    -> f.id();
                case Cancel c  -> c.id();
                case Reject r  -> r.id();
            };
            classifications.put(id, classify(e));
        }
        System.out.println();
        System.out.println("first classification by id: " + classifications.firstEntry());
        System.out.println("last classification by id:  " + classifications.lastEntry());
    }
}
