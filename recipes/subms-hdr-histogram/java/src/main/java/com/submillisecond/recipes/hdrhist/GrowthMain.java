package com.submillisecond.recipes.hdrhist;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

import com.submillisecond.perf.SubMsGrowth;

/**
 * Storage-growth capture for the HDR histogram: record an unbounded stream of
 * values and confirm the footprint does not move. The counter array is sized by
 * the largest value recorded, never by how many values were recorded, so memory
 * is O(1) in the sample count. This is a compact verdict, not a curve.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config.
 *
 * <pre>
 * printf 'rounds=20\nsignificant_digits=3\nrecords_per_round=50000\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.hdrhist.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    private GrowthMain() {}

    private static int parseInt(Map<String, String> cfg, String key, int fallback) {
        String v = cfg.get(key);
        if (v == null) {
            return fallback;
        }
        try {
            return Integer.parseInt(v.trim());
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    public static void main(String[] args) throws Exception {
        Map<String, String> cfg = new HashMap<>();
        try (BufferedReader in =
                new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                int eq = line.indexOf('=');
                if (eq > 0) {
                    cfg.put(line.substring(0, eq).trim(), line.substring(eq + 1).trim());
                }
            }
        }

        var recipe = new HdrGrowthRecipe(
                parseInt(cfg, "significant_digits", 3),
                parseInt(cfg, "rounds", 20),
                parseInt(cfg, "records_per_round", 50_000));

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(SubMsGrowth.grow(recipe, "java"), out);
        out.flush();
    }
}
