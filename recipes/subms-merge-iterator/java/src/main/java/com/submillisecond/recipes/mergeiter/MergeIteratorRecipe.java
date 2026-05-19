package com.submillisecond.recipes.mergeiter;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

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
        SubMsPerfHarness.Stage next = h.stage("next", total);
        for (int i = 0; i < total; i++) {
            long t0 = System.nanoTime();
            it.next();
            next.record(System.nanoTime() - t0);
        }

        h.meta("streams", Integer.toString(nStreams));
        h.meta("per_stream", Integer.toString(perStream));
    }
}
