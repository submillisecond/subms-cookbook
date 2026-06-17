package com.submillisecond.recipes.cuckoo;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class CuckooFilterRecipe implements SubMsRecipe {
    @Override public String name() { return "cuckoo-filter"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        CuckooFilter cf = new CuckooFilter(entries);
        for (int i = 0; i < warmup; i++) cf.insert("warm" + i);

        SubMsPerfHarness.Stage ins = h.stage("insert", entries).withKind(SubMsStageKind.HOT_PATH);
        String[] keys = new String[entries];
        for (int i = 0; i < entries; i++) {
            keys[i] = "k" + i;
            long t0 = SubMsTimer.nanosNow();
            cf.insert(keys[i]);
            ins.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage con = h.stage("contains", entries).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) {
            long t0 = SubMsTimer.nanosNow();
            cf.contains(k);
            con.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage del = h.stage("delete", entries).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) {
            long t0 = SubMsTimer.nanosNow();
            cf.delete(k);
            del.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("buckets", Integer.toString(cf.bucketCount()));
    }
}
