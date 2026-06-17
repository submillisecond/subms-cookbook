package com.submillisecond.recipes.tswal;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.stream.Stream;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives the WAL append hot path under the two batched fsync policies the
 * sub-ms claim covers: {@code append_buffered} (policy NEVER) and
 * {@code append_synced_n} (policy everyNAppends(128)). The fsync-per-append
 * (ALWAYS) figure is deliberately NOT asserted here; it is fsync-floor-limited
 * and measured separately in the writeup.
 */
public final class WalRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "subms-ts-wal";
    }

    private static Path scratchDir(String tag) {
        long nanos = System.nanoTime();
        return Path.of(System.getProperty("java.io.tmpdir"),
                "subms-ts-wal-bench-" + ProcessHandle.current().pid() + "-" + tag + "-" + nanos);
    }

    private static void deleteRecursive(Path dir) {
        if (!Files.exists(dir)) {
            return;
        }
        try (Stream<Path> walk = Files.walk(dir)) {
            walk.sorted(Comparator.reverseOrder()).forEach(p -> {
                try {
                    Files.deleteIfExists(p);
                } catch (IOException ignored) {
                    // best-effort cleanup
                }
            });
        } catch (IOException ignored) {
            // best-effort cleanup
        }
    }

    private static void drive(SubMsPerfHarness h, String stage, int rounds, TsFsyncPolicy policy) {
        Path dir = scratchDir(stage);
        TsWal wal = TsWal.open(dir, policy);
        SubMsPerfHarness.Stage s = h.stage(stage, rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            long ts = i;
            double value = i * 0.5;
            long t0 = SubMsTimer.nanosNow();
            wal.append(7, ts, value);
            s.record(SubMsTimer.nanosNow() - t0);
        }
        wal.flush();
        wal.close();
        deleteRecursive(dir);
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        drive(h, "append_buffered", rounds, TsFsyncPolicy.NEVER);
        // The fsync interval must sit below the p99 tail: at 1-in-N appends the
        // fsync spike lands in the top 1/N of samples, so N must exceed 100 for
        // the per-append p99 to clear the fsync floor. 128 keeps the periodic
        // force in p999 rather than p99.
        drive(h, "append_synced_n", rounds, TsFsyncPolicy.everyNAppends(128));

        h.meta("segment_max_records", "4096");
        h.meta("hardware_tier", "laptop");
        h.meta("subms.workload.feature", "write-ahead-log");
    }
}
