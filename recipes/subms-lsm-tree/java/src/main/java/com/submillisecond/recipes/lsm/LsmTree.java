package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

/**
 * Minimal log-structured merge tree.
 *
 * <p>Writes land in the memtable. When the memtable exceeds
 * {@code flushThresholdBytes}, it is written as a new SSTable (with a bloom
 * filter trailer) and cleared. Reads check the memtable first, then SSTables
 * newest-to-oldest; with {@link BloomMode#ON} each SSTable consults its
 * bloom filter before scanning, so misses short-circuit in a few hash probes.
 * With {@link BloomMode#OFF} the bloom probe is skipped entirely - useful
 * for measuring how much the optimisation buys you. First hit wins,
 * tombstones included.
 *
 * <p>Single-threaded by construction. No compaction, no WAL.
 */
public final class LsmTree implements AutoCloseable {

    private final Path dataDir;
    private final int flushThresholdBytes;
    private final BloomMode bloomMode;
    private final Memtable memtable = new Memtable();
    private final List<SSTable> sstables = new ArrayList<>();
    private long nextSeq;

    public LsmTree(Path dataDir, int flushThresholdBytes) throws IOException {
        this(dataDir, flushThresholdBytes, BloomMode.ON);
    }

    public LsmTree(Path dataDir, int flushThresholdBytes, BloomMode bloomMode) throws IOException {
        this.dataDir = dataDir;
        this.flushThresholdBytes = flushThresholdBytes;
        this.bloomMode = bloomMode;
        Files.createDirectories(dataDir);
        loadExisting();
    }

    public void put(String key, String value) throws IOException {
        memtable.put(key, value.getBytes(StandardCharsets.UTF_8));
        maybeFlush();
    }

    public void delete(String key) throws IOException {
        memtable.delete(key);
        maybeFlush();
    }

    public Optional<String> get(String key) throws IOException {
        Optional<Memtable.Lookup> mem = memtable.get(key);
        if (mem.isPresent()) {
            return mem.get().isTombstone()
                ? Optional.empty()
                : Optional.of(new String(mem.get().value(), StandardCharsets.UTF_8));
        }
        boolean checkBloom = bloomMode == BloomMode.ON;
        for (int i = sstables.size() - 1; i >= 0; i--) {
            Optional<SSTable.Hit> hit = sstables.get(i).get(key, checkBloom);
            if (hit.isPresent()) {
                return hit.get().isTombstone()
                    ? Optional.empty()
                    : Optional.of(new String(hit.get().value(), StandardCharsets.UTF_8));
            }
        }
        return Optional.empty();
    }

    /**
     * Every live key in {@code [lo, hi)} (a {@code null} bound is unbounded), in
     * sorted key order, as {@code (key, value)} entries. Merges the memtable over
     * every on-disk run newest-first: the newest write per key wins and
     * tombstoned keys are omitted - the same resolution as {@link #get}, across a
     * range.
     */
    public List<Map.Entry<String, String>> range(String lo, String hi) {
        // Newest source first: memtable, then runs newest -> oldest. containsKey
        // (not putIfAbsent) keeps the first value seen for a key - a TreeMap maps
        // a tombstone to null, and putIfAbsent would treat that as absent and let
        // an older run overwrite it. A null (tombstone) is dropped in the final
        // pass so a delete shadows older runs.
        TreeMap<String, byte[]> merged = new TreeMap<>();
        for (Map.Entry<String, byte[]> e : memtable.range(lo, hi)) {
            if (!merged.containsKey(e.getKey())) merged.put(e.getKey(), e.getValue());
        }
        for (int i = sstables.size() - 1; i >= 0; i--) {
            for (Map.Entry<String, byte[]> e : sstables.get(i).range(lo, hi)) {
                if (!merged.containsKey(e.getKey())) merged.put(e.getKey(), e.getValue());
            }
        }
        List<Map.Entry<String, String>> out = new ArrayList<>();
        for (Map.Entry<String, byte[]> e : merged.entrySet()) {
            if (e.getValue() != null) {
                out.add(new AbstractMap.SimpleImmutableEntry<>(
                        e.getKey(), new String(e.getValue(), StandardCharsets.UTF_8)));
            }
        }
        return out;
    }

    /** Force a flush of the current memtable, even if it is below threshold. */
    public void flush() throws IOException {
        if (memtable.isEmpty()) return;
        Path path = dataDir.resolve(String.format("sst-%012d.dat", nextSeq++));
        sstables.add(SSTable.write(path, memtable.entryCount(), memtable.sortedEntries()));
        memtable.clear();
    }

    public int sstableCount() {
        return sstables.size();
    }

    public BloomMode bloomMode() {
        return bloomMode;
    }

    @Override
    public void close() throws IOException {
        flush();
    }

    private void maybeFlush() throws IOException {
        if (memtable.approxSizeBytes() >= flushThresholdBytes) flush();
    }

    private void loadExisting() throws IOException {
        try (var stream = Files.list(dataDir)) {
            List<Path> files = stream
                .filter(p -> p.getFileName().toString().startsWith("sst-"))
                .sorted(Comparator.naturalOrder())
                .toList();
            for (Path p : files) sstables.add(SSTable.open(p));
            if (!files.isEmpty()) {
                String last = files.get(files.size() - 1).getFileName().toString();
                nextSeq = Long.parseLong(last.substring(4, last.length() - 4)) + 1;
            }
        }
    }
}
