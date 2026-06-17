package com.submillisecond.recipes.mergeiter;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class MergeIteratorRecipe implements SubMsRecipe {
    @Override public String name() { return "merge-iterator"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int nStreams = 16;
        int perStream = params.entries() / nStreams;

        List<Iterator<Long>> streams = new ArrayList<>(nStreams);
        for (int s = 0; s < nStreams; s++) {
            List<Long> data = new ArrayList<>(perStream);
            for (int i = 0; i < perStream; i++) data.add((long) (s + i * nStreams));
            streams.add(data.iterator());
        }
        MergeIterator<Long> it = new MergeIterator<>(streams);

        int total = nStreams * perStream;
        SubMsPerfHarness.Stage next = h.stage("next", total).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < total; i++) {
            long t0 = SubMsTimer.nanosNow();
            it.next();
            next.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("streams", Integer.toString(nStreams));
        h.meta("per_stream", Integer.toString(perStream));
    }
}
