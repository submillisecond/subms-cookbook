package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Optional;

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
