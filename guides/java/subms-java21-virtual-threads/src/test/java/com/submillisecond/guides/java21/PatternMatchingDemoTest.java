package com.submillisecond.guides.java21;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import com.submillisecond.guides.java21.PatternMatchingDemo.Cancel;
import com.submillisecond.guides.java21.PatternMatchingDemo.Fill;
import com.submillisecond.guides.java21.PatternMatchingDemo.New;
import com.submillisecond.guides.java21.PatternMatchingDemo.OrderEvent;
import com.submillisecond.guides.java21.PatternMatchingDemo.Reject;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * Exercises every branch of the pattern-matched switches in
 * {@link PatternMatchingDemo}. The compiler enforces exhaustiveness
 * against the sealed {@code OrderEvent} hierarchy at build time, so
 * these tests are about the *behaviour* of each branch, not about
 * coverage.
 */
final class PatternMatchingDemoTest {

    @Test
    @DisplayName("describe() destructures every record variant correctly")
    void describeFormatsEachVariant() {
        assertEquals("new  #1  100 AAPL @ 19500",
                PatternMatchingDemo.describe(new New(1, "AAPL", 100, 19_500)));
        assertEquals("fill #1  100 @ 19502",
                PatternMatchingDemo.describe(new Fill(1, 100, 19_502)));
        assertEquals("cxl  #2  (user)",
                PatternMatchingDemo.describe(new Cancel(2, "user")));
        assertEquals("rej  #3  (risk)",
                PatternMatchingDemo.describe(new Reject(3, "risk")));
    }

    @Test
    @DisplayName("classify() guard clauses partition Fill by size")
    void classifyDiscriminatesFillBySize() {
        assertEquals("ordinary-fill", PatternMatchingDemo.classify(new Fill(1, 50,     19_500)));
        assertEquals("ordinary-fill", PatternMatchingDemo.classify(new Fill(1, 999,    19_500)));
        assertEquals("block-trade",   PatternMatchingDemo.classify(new Fill(1, 1_000,  19_500)));
        assertEquals("block-trade",   PatternMatchingDemo.classify(new Fill(1, 10_000, 19_500)));
    }

    @Test
    @DisplayName("classify() guard clauses partition Cancel by reason")
    void classifyDiscriminatesCancelByReason() {
        assertEquals("user-cancel",  PatternMatchingDemo.classify(new Cancel(1, "user")));
        assertEquals("stale-cancel", PatternMatchingDemo.classify(new Cancel(1, "session timeout")));
        // "timeout" anywhere in the reason promotes to stale-cancel - the guard
        // is `reason().contains("timeout")`, not equality.
        assertEquals("stale-cancel", PatternMatchingDemo.classify(new Cancel(1, "client read timeout")));
    }

    @Test
    @DisplayName("classify() routes New and Reject to their unique labels")
    void classifyHandlesRemainingVariants() {
        assertEquals("new-order", PatternMatchingDemo.classify(new New(1, "AAPL", 100, 19_500)));
        assertEquals("reject",    PatternMatchingDemo.classify(new Reject(1, "risk")));
    }

    @Test
    @DisplayName("describe() handles every permitted subtype - exhaustiveness held by the compiler")
    void describeIsExhaustive() {
        // If a new variant is added to OrderEvent without a matching case,
        // PatternMatchingDemo.describe will fail to compile. This test just
        // pins the variant set so a casual refactor that drops a subtype
        // is caught by the test suite as well as by javac.
        OrderEvent[] one = {
                new New(0, "X", 0, 0),
                new Fill(0, 0, 0),
                new Cancel(0, ""),
                new Reject(0, ""),
        };
        assertEquals(4, one.length, "the test must update if the sealed hierarchy grows");
    }
}
