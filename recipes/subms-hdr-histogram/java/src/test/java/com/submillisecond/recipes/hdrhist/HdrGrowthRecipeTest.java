package com.submillisecond.recipes.hdrhist;

import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsGrowth;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The growth capture must report the counter array the histogram actually
 * holds. It used to publish a closed-form 17k-counter estimate instead, which
 * under-reported this workload's allocation by 2.4x on both ports at once - the
 * two curves agreed with each other and with nothing else.
 */
final class HdrGrowthRecipeTest {

    /**
     * Values top out just under 1e9, which lands at index 40678, so the array
     * holds 40679 u64 counters.
     */
    private static final long WORKLOAD_FOOTPRINT_BYTES = 325_432L;

    /** What the harness used to publish. */
    private static final long OLD_ESTIMATE_BYTES = 136_000L;

    private static void runRound(HdrGrowthRecipe recipe, int ops) {
        for (int i = 0; i < ops; i++) {
            recipe.op(0, i);
        }
    }

    @Test
    void reportsTheArrayTheHistogramActuallyHolds() {
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 2, 5_000);
        runRound(recipe, 5_000);
        assertEquals(recipe.memoryBytes(), recipe.liveBytes());
        assertEquals(0, recipe.memoryBytes() % 8, "the array is long counters");
    }

    @Test
    void theWorkloadFootprintIsTheFullCounterArray() {
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 1, 50_000);
        runRound(recipe, 50_000);
        assertEquals(WORKLOAD_FOOTPRINT_BYTES, recipe.memoryBytes());
        assertNotEquals(OLD_ESTIMATE_BYTES, recipe.memoryBytes());
    }

    @Test
    void footprintIsFlatInTheSampleCount() {
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 3, 50_000);
        runRound(recipe, 50_000);
        long afterOne = recipe.memoryBytes();
        runRound(recipe, 200_000);
        assertEquals(afterOne, recipe.memoryBytes());
        assertEquals(250_000L, recipe.structures().get("records"));
    }

    @Test
    void theBoundCoversTheTopOfTheValueRange() {
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 1, 1);
        assertEquals(SubMsGrowth.GrowthClass.BOUNDED, recipe.expectedClass());
        assertTrue(
            recipe.expectedBound() >= WORKLOAD_FOOTPRINT_BYTES,
            "bound " + recipe.expectedBound() + " is under the array the workload allocates");
    }

    @Test
    void aWiderPrecisionNeedsAWiderBound() {
        double three = new HdrGrowthRecipe(3, 1, 1).expectedBound();
        double four = new HdrGrowthRecipe(4, 1, 1).expectedBound();
        assertTrue(four > three, "4 significant digits must bound higher");
    }

    @Test
    void theRecipeReportsItsShape() {
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 7, 11);
        assertEquals("subms-hdr-histogram", recipe.name());
        assertEquals("record", recipe.opName());
        assertEquals(7, recipe.rounds());
        assertEquals(11, recipe.opsPerRound());
        assertTrue(recipe.compact());
        assertTrue(recipe.structures().containsKey("records"));
    }

    @Test
    void bothPortsReportTheSameFootprint() {
        // The growth JSON is compared across languages, so the two ports must
        // agree on the number: counters * 8, with no JVM array header added.
        HdrGrowthRecipe recipe = new HdrGrowthRecipe(3, 1, 50_000);
        runRound(recipe, 50_000);
        HdrHistogram direct = new HdrHistogram(3);
        direct.record(999_999_999L);
        assertEquals(direct.footprintBytes(), recipe.memoryBytes());
    }
}
