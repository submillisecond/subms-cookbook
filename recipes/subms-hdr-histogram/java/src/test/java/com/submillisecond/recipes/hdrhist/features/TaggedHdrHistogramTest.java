package com.submillisecond.recipes.hdrhist.features;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

final class TaggedHdrHistogramTest {

    @Test
    void emptyReturnsZero() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        assertEquals(0, h.count());
        assertEquals(0, h.countForTag((byte) 1));
        assertEquals(0, h.valueAtPercentile(0.99));
        assertEquals(0, h.valueAtPercentileForTag(0.99, (byte) 1));
        assertEquals(0, h.max());
    }

    @Test
    void recordsTaggedCounts() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (long v = 1; v <= 10; v++) h.record(v, (byte) 1);
        for (long v = 100; v <= 109; v++) h.record(v, (byte) 2);
        assertEquals(20, h.count());
        assertEquals(10, h.countForTag((byte) 1));
        assertEquals(10, h.countForTag((byte) 2));
        assertEquals(0, h.countForTag((byte) 3));
    }

    @Test
    void perTagPercentilesAreSeparate() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (long v = 1; v <= 1000; v++) h.record(v, (byte) 1);
        for (long v = 10_000; v <= 11_000; v++) h.record(v, (byte) 2);
        long p99a = h.valueAtPercentileForTag(0.99, (byte) 1);
        long p99b = h.valueAtPercentileForTag(0.99, (byte) 2);
        assertTrue(p99a < 1100, "tag 1 small, got " + p99a);
        assertTrue(p99b >= 10_000, "tag 2 large, got " + p99b);
    }

    @Test
    void aggregatePercentileSpansAllTags() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (long v = 1; v <= 500; v++) h.record(v, (byte) 1);
        for (long v = 501; v <= 1000; v++) h.record(v, (byte) 2);
        assertEquals(1000, h.count());
        long p50 = h.valueAtPercentile(0.5);
        assertTrue(p50 >= 450 && p50 <= 550, "aggregate p50=" + p50);
    }

    @Test
    void tagsListingReturnsUnique() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        h.record(10, (byte) 1);
        h.record(20, (byte) 2);
        h.record(30, (byte) 1);
        h.record(40, (byte) 3);
        List<Byte> tags = h.tags();
        assertEquals(List.of((byte) 1, (byte) 2, (byte) 3), tags);
    }

    @Test
    void unknownTagPercentileIsZero() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        h.record(50, (byte) 1);
        assertEquals(0, h.valueAtPercentileForTag(0.5, (byte) 99));
    }

    @Test
    void manyTagsPerBucket() {
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (byte tag = 0; tag < 10; tag++) {
            for (int i = 0; i < 100; i++) h.record(50, tag);
        }
        assertEquals(1000, h.count());
        for (byte tag = 0; tag < 10; tag++) {
            assertEquals(100, h.countForTag(tag));
            assertTrue(h.valueAtPercentileForTag(0.99, tag) > 0);
        }
    }
}
