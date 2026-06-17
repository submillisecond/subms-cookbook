package com.submillisecond.recipes.blockcache.features;

import java.util.ArrayList;
import java.util.ArrayDeque;
import java.util.HashMap;
import java.util.Map;

/**
 * W-TinyLFU admission policy (Einziger + Friedman + Manasse, 2017).
 *
 * <p>Cache layout:
 * <ul>
 *   <li>Window (1% of capacity) - LRU; new keys enter here.</li>
 *   <li>Main protected (80% of main) - LRU; the popular working set.</li>
 *   <li>Main probation (20% of main) - LRU; recent demotees + admits.</li>
 * </ul>
 *
 * <p>Window eviction yields a candidate that competes against the
 * probation LRU victim via a frequency estimator (count-min sketch).
 * Candidate admitted iff freq(candidate) &gt;= freq(victim).
 *
 * <p>The CMS implementation lives in this file rather than depending
 * on the sibling {@code subms-count-min-sketch} recipe. Reason: the
 * cache is a leaf in the dep graph; pulling in a sibling recipe would
 * create a cyclic-refresh risk on per-recipe releases and force
 * consumers to pull two artefacts for one feature. The local CMS is
 * 4 rows of 4-bit counters keyed by FNV-1a + seeded multiply.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_block_cache::features::tinylfu::TinyLfuCache}.
 */
public final class TinyLfuCache<K, V> {

    private static final int CMS_ROWS = 4;
    private static final int CMS_MAX = 15;
    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME = 0x100000001b3L;
    private static final long[] SEEDS = new long[] {
        0x9e3779b97f4a7c15L,
        0xbf58476d1ce4e5b9L,
        0x94d049bb133111ebL,
        0x2545f4914f6cdd1dL,
    };

    private enum Segment { WINDOW, PROBATION, PROTECTED }

    private static final class Node<K, V> {
        K key;
        V value;
        int prev = -1;
        int next = -1;
        Segment seg;
        Node(K k, V v, Segment s) { this.key = k; this.value = v; this.seg = s; }
    }

    public record Evicted<K, V>(K key, V value) {}

    private final int c;
    private final int windowCap;
    private final int protectedCap;
    private final int probationCap;
    private final ArrayList<Node<K, V>> nodes = new ArrayList<>();
    private final ArrayDeque<Integer> free = new ArrayDeque<>();
    private final Map<K, Integer> index = new HashMap<>();
    private int wHead = -1, wTail = -1, wLen;
    private int pHead = -1, pTail = -1, pLen;
    private int rHead = -1, rTail = -1, rLen;
    private final Cms cms;
    private final Doorkeeper doorkeeper;
    private long admissions;
    private long rejections;

    public TinyLfuCache(int capacity) {
        int cc = Math.max(4, capacity);
        this.c = cc;
        this.windowCap = Math.max(1, cc / 100);
        int main = cc - windowCap;
        this.protectedCap = Math.max(1, (main * 4) / 5);
        this.probationCap = Math.max(1, main - protectedCap);
        int cmsCells = Math.max(64, cc * 4);
        long sampleSize = Math.max(64L, 10L * cc);
        this.cms = new Cms(cmsCells, sampleSize);
        this.doorkeeper = new Doorkeeper(cc * 8);
    }

    public int capacity() { return c; }
    public int size() { return wLen + pLen + rLen; }
    public boolean isEmpty() { return size() == 0; }
    public long admissions() { return admissions; }
    public long rejections() { return rejections; }
    public int windowLen() { return wLen; }
    public int protectedLen() { return pLen; }
    public int probationLen() { return rLen; }

    private void recordAccess(long hash) {
        if (doorkeeper.checkOrAdd(hash)) {
            cms.increment(hash);
            if (cms.additions == 0) {
                doorkeeper.clear();
            }
        }
    }

    public V get(K key) {
        long h = hash(key);
        recordAccess(h);
        Integer boxed = index.get(key);
        if (boxed == null) return null;
        int id = boxed;
        Node<K, V> n = nodes.get(id);
        switch (n.seg) {
            case WINDOW -> {
                unlink(id);
                pushFrontWindow(id);
            }
            case PROBATION -> {
                unlink(id);
                rLen--;
                if (pLen >= protectedCap) {
                    int demote = pTail;
                    unlink(demote);
                    pLen--;
                    nodes.get(demote).seg = Segment.PROBATION;
                    pushFrontProbation(demote);
                    rLen++;
                }
                n.seg = Segment.PROTECTED;
                pushFrontProtected(id);
                pLen++;
            }
            case PROTECTED -> {
                unlink(id);
                pushFrontProtected(id);
            }
        }
        return nodes.get(id).value;
    }

    public Evicted<K, V> put(K key, V value) {
        long h = hash(key);
        recordAccess(h);

        Integer existing = index.get(key);
        if (existing != null) {
            int id = existing;
            Node<K, V> n = nodes.get(id);
            n.value = value;
            Segment seg = n.seg;
            unlink(id);
            switch (seg) {
                case WINDOW -> pushFrontWindow(id);
                case PROBATION -> pushFrontProbation(id);
                case PROTECTED -> pushFrontProtected(id);
            }
            return null;
        }

        if (wLen < windowCap) {
            int id = alloc(new Node<>(key, value, Segment.WINDOW));
            index.put(key, id);
            pushFrontWindow(id);
            wLen++;
            return null;
        }

        int candidateId = wTail;
        unlink(candidateId);
        wLen--;

        int newId = alloc(new Node<>(key, value, Segment.WINDOW));
        index.put(key, newId);
        pushFrontWindow(newId);
        wLen++;

        if (rLen < probationCap) {
            nodes.get(candidateId).seg = Segment.PROBATION;
            pushFrontProbation(candidateId);
            rLen++;
            admissions++;
            return null;
        }

        int victimId = rTail;
        long candHash = hash(nodes.get(candidateId).key);
        long vicHash = hash(nodes.get(victimId).key);
        int candFreq = cms.estimate(candHash);
        int vicFreq = cms.estimate(vicHash);

        if (candFreq >= vicFreq) {
            unlink(victimId);
            rLen--;
            Node<K, V> v = nodes.set(victimId, null);
            index.remove(v.key);
            free.push(victimId);

            nodes.get(candidateId).seg = Segment.PROBATION;
            pushFrontProbation(candidateId);
            rLen++;
            admissions++;
            return new Evicted<>(v.key, v.value);
        } else {
            Node<K, V> cand = nodes.set(candidateId, null);
            index.remove(cand.key);
            free.push(candidateId);
            rejections++;
            return new Evicted<>(cand.key, cand.value);
        }
    }

    private int alloc(Node<K, V> n) {
        if (!free.isEmpty()) {
            int id = free.pop();
            nodes.set(id, n);
            return id;
        }
        int id = nodes.size();
        nodes.add(n);
        return id;
    }

    private void unlink(int id) {
        Node<K, V> n = nodes.get(id);
        int prev = n.prev, next = n.next;
        Segment seg = n.seg;
        if (prev != -1) nodes.get(prev).next = next;
        if (next != -1) nodes.get(next).prev = prev;
        n.prev = -1; n.next = -1;
        switch (seg) {
            case WINDOW -> {
                if (wHead == id) wHead = next;
                if (wTail == id) wTail = prev;
            }
            case PROBATION -> {
                if (rHead == id) rHead = next;
                if (rTail == id) rTail = prev;
            }
            case PROTECTED -> {
                if (pHead == id) pHead = next;
                if (pTail == id) pTail = prev;
            }
        }
    }

    private void pushFrontWindow(int id) {
        int old = wHead;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        wHead = id;
        if (wTail == -1) wTail = id;
    }
    private void pushFrontProtected(int id) {
        int old = pHead;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        pHead = id;
        if (pTail == -1) pTail = id;
    }
    private void pushFrontProbation(int id) {
        int old = rHead;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        rHead = id;
        if (rTail == -1) rTail = id;
    }

    private long hash(K key) {
        // FNV-1a over key.toString() so any K type produces a stable hash.
        String s = key.toString();
        long h = FNV_OFFSET;
        for (int i = 0; i < s.length(); i++) {
            h ^= s.charAt(i);
            h *= FNV_PRIME;
        }
        return h;
    }

    /** Count-min sketch. 4 rows, 4-bit counters, periodic aging. */
    static final class Cms {
        final int cols;
        final byte[][] rows;
        long additions;
        final long sampleSize;

        Cms(int cells, long sampleSize) {
            int cols = nextPowerOfTwo(Math.max(8, cells));
            this.cols = cols;
            this.rows = new byte[CMS_ROWS][(cols + 1) / 2];
            this.sampleSize = sampleSize;
        }

        int idx(int row, long h) {
            long seeded = h * SEEDS[row] + (SEEDS[row] >>> 17);
            return (int) (seeded & (cols - 1));
        }

        int read(int row, int idx) {
            byte b = rows[row][idx / 2];
            return ((idx & 1) == 0) ? (b & 0x0f) : ((b >> 4) & 0x0f);
        }
        void write(int row, int idx, int v) {
            int i = idx / 2;
            int vv = v & 0x0f;
            if ((idx & 1) == 0) {
                rows[row][i] = (byte) ((rows[row][i] & 0xf0) | vv);
            } else {
                rows[row][i] = (byte) ((rows[row][i] & 0x0f) | (vv << 4));
            }
        }

        void increment(long hash) {
            boolean added = false;
            for (int r = 0; r < CMS_ROWS; r++) {
                int idx = idx(r, hash);
                int cur = read(r, idx);
                if (cur < CMS_MAX) {
                    write(r, idx, cur + 1);
                    added = true;
                }
            }
            if (added) {
                additions++;
                if (additions >= sampleSize) {
                    reset();
                }
            }
        }

        int estimate(long hash) {
            int m = Integer.MAX_VALUE;
            for (int r = 0; r < CMS_ROWS; r++) {
                int v = read(r, idx(r, hash));
                if (v < m) m = v;
            }
            return m;
        }

        void reset() {
            for (int r = 0; r < CMS_ROWS; r++) {
                for (int i = 0; i < rows[r].length; i++) {
                    int b = rows[r][i] & 0xff;
                    int lo = (b & 0x0f) >> 1;
                    int hi = ((b >> 4) & 0x0f) >> 1;
                    rows[r][i] = (byte) ((hi << 4) | lo);
                }
            }
            additions /= 2;
        }
    }

    /** Doorkeeper bloom filter, 2-hash. */
    static final class Doorkeeper {
        final long[] bits;
        final long mask;
        Doorkeeper(int requested) {
            int words = Math.max(1, nextPowerOfTwo(Math.max(64, requested)) / 64);
            this.bits = new long[words];
            this.mask = ((long) words * 64) - 1;
        }
        int idx(long hash, long salt) {
            return (int) ((hash * salt) & mask);
        }
        boolean checkOrAdd(long hash) {
            int p1 = idx(hash, SEEDS[0]);
            int p2 = idx(hash, SEEDS[1]);
            boolean allSet = true;
            int w1 = p1 >> 6;
            long b1 = 1L << (p1 & 63);
            if ((bits[w1] & b1) == 0) { allSet = false; bits[w1] |= b1; }
            int w2 = p2 >> 6;
            long b2 = 1L << (p2 & 63);
            if ((bits[w2] & b2) == 0) { allSet = false; bits[w2] |= b2; }
            return allSet;
        }
        void clear() { for (int i = 0; i < bits.length; i++) bits[i] = 0L; }
    }

    private static int nextPowerOfTwo(int n) {
        int r = 1;
        while (r < n) r <<= 1;
        return r;
    }
}
