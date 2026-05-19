package com.submillisecond.recipes.segment;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

public final class SegmentReaderRecipe implements SubMsRecipe {
    @Override public String name() { return "segment-reader"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        ByteArrayOutputStream baos = new ByteArrayOutputStream(entries * 32);
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            SegmentWriter w = new SegmentWriter(dos);
            for (int i = 0; i < entries; i++) {
                w.write(("record-" + i).getBytes());
            }
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        byte[] segment = baos.toByteArray();

        SegmentReader r = new SegmentReader(new DataInputStream(new ByteArrayInputStream(segment)));
        SubMsPerfHarness.Stage stage = h.stage("next_record", entries);
        try {
            for (int i = 0; i < entries; i++) {
                long t0 = System.nanoTime();
                r.nextRecord();
                stage.record(System.nanoTime() - t0);
            }
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }

        h.meta("segment_bytes", Integer.toString(segment.length));
    }
}
