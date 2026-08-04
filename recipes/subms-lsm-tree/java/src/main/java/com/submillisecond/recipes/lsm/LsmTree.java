package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.AbstractMap;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import java.util.concurrent.atomic.AtomicReference;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Log-structured merge tree with background flush.
 *
 * <p>Writes land in an in-memory {@code active} memtable. When it exceeds
 * {@code flushThresholdBytes} the tree <em>rotates</em>: the full memtable is
 * frozen and a fresh one is installed so the triggering write returns
 * immediately. By default ({@link FlushMode#BACKGROUND}) a background thread
 * turns each frozen memtable into an SSTable (with a bloom-filter trailer) off
 * the write path, so {@code put} never pays the O(memtable) flush cost - only
 * the swap. Reads check {@code active}, then the frozen memtables still awaiting
 * flush (newest first), then SSTables newest-to-oldest; with {@link BloomMode#ON}
 * each SSTable consults its bloom filter before scanning, so misses short-circuit
 * in a few hash probes. First hit wins, tombstones included.
 *
 * <p>{@link FlushMode#SYNC} flushes inline on the calling thread instead -
 * thread-free and deterministic (single-threaded targets, deterministic replay)
 * at the cost of a periodic write-latency spike on the write that triggers a
 * flush.
 *
 * <p>Durability: a frozen memtable queued for flush is not on disk until the
 * worker writes it, so a hard crash loses the queued + active memtables unless
 * the WAL feature is recording them for replay - the same no-durability-without-
 * WAL profile as before, just with a slightly wider in-memory window.
 * {@link #flush()} forces everything pending to disk and blocks until it is;
 * {@link #close()} does the same and stops the worker.
 *
 * <p>Single external writer by construction: as with the previous version, one
 * thread drives {@code put}/{@code get}. The only concurrency is the internal
 * flush worker, coordinated by the lock below.
 */
public final class LsmTree implements AutoCloseable {

    private static final int DEFAULT_MAX_IMMUTABLE = 4;

    private final Path dataDir;
    private final int flushThresholdBytes;
    private final BloomMode bloomMode;

    /** Writer-local buffer of pending writes. The hot {@code put} path never locks. */
    private Memtable active = new Memtable();

    private final ReentrantLock lock = new ReentrantLock();
    private final Condition signal = lock.newCondition();

    // ---- guarded by lock ----
    /** Frozen memtables awaiting flush, oldest at the head; reads still see them. */
    private final Deque<Memtable> immutable = new ArrayDeque<>();
    private long nextSeq;
    private boolean shutdown;
    private IOException flushErr;

    /**
     * On-disk runs, oldest -> newest, published as one immutable list so a reader
     * grabs the whole snapshot with a single volatile read; registering a run
     * swaps in a new list (copy-on-write). Reads never lock for this.
     */
    private final AtomicReference<List<SSTable>> sstablesRef = new AtomicReference<>(List.of());

    private FlushMode flushMode = FlushMode.BACKGROUND;
    /** Run count that triggers an automatic full merge; 0 disables it. */
    private int compactionTrigger;
    /** Lazily spawned on the first background flush; {@code null} in SYNC mode. */
    private Thread worker;

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

    /**
     * Choose inline vs background flush. {@link FlushMode#BACKGROUND} is the
     * default; call this before the first write to opt into {@link FlushMode#SYNC}.
     * Returns {@code this} for builder-style construction.
     */
    public LsmTree setFlushMode(FlushMode mode) {
        this.flushMode = mode;
        return this;
    }

    /** The active flush mode. */
    public FlushMode flushMode() {
        return flushMode;
    }

    /**
     * Enable automatic compaction: once the tree accumulates {@code trigger}
     * on-disk runs, the next flush merges them all into one, dropping every
     * superseded version and tombstone. {@code trigger = 0} disables it (the
     * default). This is what bounds on-disk size under overwrite-heavy
     * workloads - without it, every flush leaves a fresh run and the dead
     * versions in older runs are never reclaimed. Returns {@code this} for
     * builder-style construction.
     */
    public LsmTree setCompactionTrigger(int trigger) {
        this.compactionTrigger = trigger;
        return this;
    }

    /** The current auto-compaction trigger (0 = disabled). */
    public int compactionTrigger() {
        return compactionTrigger;
    }

    /**
     * Merge every on-disk run into a single run, keeping only the newest value
     * per key and discarding superseded versions and tombstones. Safe to call
     * manually at any time; a no-op when there are fewer than two runs. Drains
     * any pending background flush first so the merge sees every run.
     */
    public void compact() throws IOException {
        enqueueActive();
        drain();

        List<SSTable> ssts = sstablesRef.get();
        if (ssts.size() < 2) return;

        // Runs are ordered oldest -> newest, so a later run's value for a key
        // wins. A full merge has no older run left to shadow, so a tombstone
        // just drops the key entirely.
        TreeMap<String, byte[]> merged = new TreeMap<>();
        for (SSTable sst : ssts) {
            for (Map.Entry<String, byte[]> e : sst.range(null, null)) {
                merged.put(e.getKey(), e.getValue());
            }
        }
        List<Map.Entry<String, byte[]>> live = new ArrayList<>(merged.size());
        for (Map.Entry<String, byte[]> e : merged.entrySet()) {
            if (e.getValue() != null) live.add(e);
        }

        long seq;
        lock.lock();
        try {
            seq = nextSeq++;
        } finally {
            lock.unlock();
        }
        Path path = dataDir.resolve(String.format("sst-%012d.dat", seq));
        SSTable compacted = SSTable.write(path, live.size(), live);

        // No flush can have raced us: we drained, and this tree is the only
        // writer, so `immutable` stayed empty and no new run was appended.
        sstablesRef.set(List.of(compacted));
        for (SSTable old : ssts) {
            Files.deleteIfExists(old.path());
        }
    }

    public void put(String key, String value) throws IOException {
        active.put(key, value.getBytes(StandardCharsets.UTF_8));
        maybeRotate();
    }

    public void delete(String key) throws IOException {
        active.delete(key);
        maybeRotate();
    }

    public Optional<String> get(String key) throws IOException {
        Optional<Memtable.Lookup> mem = active.get(key);
        if (mem.isPresent()) {
            return decode(mem.get());
        }
        for (Memtable m : snapshotImmutable()) {
            Optional<Memtable.Lookup> h = m.get(key);
            if (h.isPresent()) {
                return decode(h.get());
            }
        }
        boolean checkBloom = bloomMode == BloomMode.ON;
        List<SSTable> ssts = sstablesRef.get();
        for (int i = ssts.size() - 1; i >= 0; i--) {
            Optional<SSTable.Hit> hit = ssts.get(i).get(key, checkBloom);
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
     * sorted key order, as {@code (key, value)} entries. Merges the active memtable
     * over the frozen memtables and every on-disk run newest-first: the newest
     * write per key wins and tombstoned keys are omitted - the same resolution as
     * {@link #get}, across a range.
     */
    public List<Map.Entry<String, String>> range(String lo, String hi) {
        // Newest source first: active memtable, frozen memtables newest -> oldest,
        // then runs newest -> oldest. containsKey (not putIfAbsent) keeps the first
        // value seen for a key - a TreeMap maps a tombstone to null, and putIfAbsent
        // would treat that as absent and let an older source overwrite it. A null
        // (tombstone) is dropped in the final pass so a delete shadows older sources.
        TreeMap<String, byte[]> merged = new TreeMap<>();
        for (Map.Entry<String, byte[]> e : active.range(lo, hi)) {
            if (!merged.containsKey(e.getKey())) merged.put(e.getKey(), e.getValue());
        }
        for (Memtable m : snapshotImmutable()) {
            for (Map.Entry<String, byte[]> e : m.range(lo, hi)) {
                if (!merged.containsKey(e.getKey())) merged.put(e.getKey(), e.getValue());
            }
        }
        List<SSTable> ssts = sstablesRef.get();
        for (int i = ssts.size() - 1; i >= 0; i--) {
            for (Map.Entry<String, byte[]> e : ssts.get(i).range(lo, hi)) {
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

    /**
     * Force everything pending - the active memtable and any frozen memtables
     * still queued - to disk, blocking until it is registered as SSTables. The
     * deterministic sync point tests and callers rely on.
     */
    public void flush() throws IOException {
        enqueueActive();
        drain();
    }

    public int sstableCount() {
        return sstablesRef.get().size();
    }

    public BloomMode bloomMode() {
        return bloomMode;
    }

    @Override
    public void close() throws IOException {
        if (worker != null) {
            enqueueActive();
            lock.lock();
            try {
                shutdown = true;
                signal.signalAll();
            } finally {
                lock.unlock();
            }
            try {
                worker.join();
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            worker = null;
            throwIfFlushErr();
        } else {
            flushInline();
        }
    }

    // ---- internals ----

    private static Optional<String> decode(Memtable.Lookup l) {
        return l.isTombstone()
            ? Optional.empty()
            : Optional.of(new String(l.value(), StandardCharsets.UTF_8));
    }

    /** Frozen memtables newest-first; the lock is held only to copy the few refs. */
    private List<Memtable> snapshotImmutable() {
        lock.lock();
        try {
            if (immutable.isEmpty()) return List.of();
            List<Memtable> out = new ArrayList<>(immutable);
            Collections.reverse(out);
            return out;
        } finally {
            lock.unlock();
        }
    }

    private void maybeRotate() throws IOException {
        if (active.approxSizeBytes() < flushThresholdBytes) return;
        enqueueActive();
        // Opt-in auto-compaction: bound the run count (and reclaim dead versions)
        // once it reaches the trigger. `compact` drains first, so a background
        // flush still in flight is accounted for.
        if (compactionTrigger > 0 && sstableCount() >= compactionTrigger) {
            compact();
        }
    }

    /**
     * Move the active memtable into the flush pipeline and install a fresh one.
     * Background mode enqueues it for the worker (spawning it on first use); Sync
     * mode writes the SSTable inline on this thread.
     */
    private void enqueueActive() throws IOException {
        if (active.isEmpty()) {
            throwIfFlushErr();
            return;
        }
        if (flushMode == FlushMode.SYNC) {
            flushInline();
            return;
        }
        ensureWorker();
        Memtable frozen = active;
        active = new Memtable();
        lock.lock();
        try {
            if (flushErr != null) {
                throw takeFlushErr();
            }
            while (immutable.size() >= DEFAULT_MAX_IMMUTABLE && !shutdown) {
                signal.awaitUninterruptibly();
            }
            immutable.addLast(frozen);
            signal.signalAll();
        } finally {
            lock.unlock();
        }
    }

    /** Synchronous flush of the active memtable on the calling thread. */
    private void flushInline() throws IOException {
        if (active.isEmpty()) return;
        long seq;
        lock.lock();
        try {
            seq = nextSeq++;
        } finally {
            lock.unlock();
        }
        Path path = dataDir.resolve(String.format("sst-%012d.dat", seq));
        SSTable sst = SSTable.write(path, active.entryCount(), active.sortedEntries());
        active.clear();
        registerSstable(sst);
    }

    /** Block until the frozen-memtable queue is empty (background mode only). */
    private void drain() throws IOException {
        if (worker == null) return;
        lock.lock();
        try {
            while (!immutable.isEmpty() && flushErr == null) {
                signal.awaitUninterruptibly();
            }
            if (flushErr != null) {
                throw takeFlushErr();
            }
        } finally {
            lock.unlock();
        }
    }

    private void throwIfFlushErr() throws IOException {
        lock.lock();
        try {
            if (flushErr != null) throw takeFlushErr();
        } finally {
            lock.unlock();
        }
    }

    /** Caller must hold {@code lock}. */
    private IOException takeFlushErr() {
        IOException e = flushErr;
        flushErr = null;
        return e;
    }

    private void registerSstable(SSTable sst) {
        List<SSTable> cur = sstablesRef.get();
        List<SSTable> next = new ArrayList<>(cur.size() + 1);
        next.addAll(cur);
        next.add(sst);
        sstablesRef.set(List.copyOf(next));
    }

    private void ensureWorker() {
        if (worker != null) return;
        worker = new Thread(this::flushWorker, "subms-lsm-flush");
        worker.setDaemon(true);
        worker.start();
    }

    /**
     * Background flush loop: turn each frozen memtable into an SSTable off the
     * write path. The memtable stays in {@code immutable} (visible to readers)
     * until its SSTable is registered, and the register + pop happen under one
     * lock, so a key is never transiently invisible during the hand-off.
     */
    private void flushWorker() {
        while (true) {
            Memtable frozen;
            lock.lock();
            try {
                while (immutable.isEmpty() && !shutdown) {
                    signal.awaitUninterruptibly();
                }
                if (immutable.isEmpty() && shutdown) return;
                frozen = immutable.peekFirst();
            } finally {
                lock.unlock();
            }

            long seq;
            lock.lock();
            try {
                seq = nextSeq++;
            } finally {
                lock.unlock();
            }
            Path path = dataDir.resolve(String.format("sst-%012d.dat", seq));
            SSTable written = null;
            IOException error = null;
            try {
                written = SSTable.write(path, frozen.entryCount(), frozen.sortedEntries());
            } catch (IOException e) {
                error = e;
            }

            lock.lock();
            try {
                if (written != null) {
                    registerSstable(written);
                } else if (flushErr == null) {
                    flushErr = error;
                }
                immutable.pollFirst();
                signal.signalAll();
            } finally {
                lock.unlock();
            }
        }
    }

    private void loadExisting() throws IOException {
        try (var stream = Files.list(dataDir)) {
            List<Path> files = stream
                .filter(p -> p.getFileName().toString().startsWith("sst-"))
                .sorted(Comparator.naturalOrder())
                .toList();
            List<SSTable> loaded = new ArrayList<>(files.size());
            for (Path p : files) loaded.add(SSTable.open(p));
            sstablesRef.set(List.copyOf(loaded));
            if (!files.isEmpty()) {
                String last = files.get(files.size() - 1).getFileName().toString();
                nextSeq = Long.parseLong(last.substring(4, last.length() - 4)) + 1;
            }
        }
    }
}
