package com.submillisecond.recipes.eventsaga;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Random;

import org.junit.jupiter.api.Test;

/** The saga compensation invariant must hold for any step count + failure point. */
class PropertyTest {
    private static final SagaAction OK = () -> {};

    @Test
    void propCompensationInvariant() {
        Random rng = new Random(11);
        for (int it = 0; it < 1000; it++) {
            int n = 1 + rng.nextInt(8);
            Integer fail = rng.nextBoolean() ? rng.nextInt(n) : null;
            Saga s = new Saga("p");
            for (int i = 0; i < n; i++) {
                boolean shouldFail = fail != null && fail == i;
                s.step("s" + i, shouldFail ? () -> {
                    throw new RuntimeException("x");
                } : OK, OK);
            }
            SagaReport r = s.run();
            if (fail == null) {
                assertEquals(Outcome.COMMITTED, r.outcome());
                List<String> all = new ArrayList<>();
                for (int i = 0; i < n; i++) {
                    all.add("s" + i);
                }
                assertEquals(all, r.forwardRan());
                assertTrue(r.compensated().isEmpty());
            } else {
                assertEquals(Outcome.COMPENSATED, r.outcome());
                assertEquals("s" + fail, r.failedStep());
                List<String> ran = new ArrayList<>();
                for (int i = 0; i < fail; i++) {
                    ran.add("s" + i);
                }
                assertEquals(ran, r.forwardRan());
                List<String> rev = new ArrayList<>(ran);
                Collections.reverse(rev);
                assertEquals(rev, r.compensated());
            }
        }
    }
}
