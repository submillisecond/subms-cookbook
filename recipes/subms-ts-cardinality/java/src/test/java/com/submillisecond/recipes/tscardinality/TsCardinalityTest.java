package com.submillisecond.recipes.tscardinality;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

class TsCardinalityTest {

    // ---------- TsCardinalityGuard ----------

    @Test
    void guardAdmitsUpToMaxThenRejects() {
        TsCardinalityGuard g = new TsCardinalityGuard(3, TsOverflowPolicy.REJECT);
        g.admit();
        g.admit();
        g.admit();
        assertEquals(3, g.count());
        TsCardinalityException ex = assertThrows(TsCardinalityException.class, g::admit);
        assertEquals(TsCardinalityException.Kind.CARDINALITY_CAP, ex.kind());
        assertEquals(3, ex.max());
        assertEquals(3, g.count()); // rejected admit leaves the count untouched
    }

    @Test
    void guardAllowPolicyAdmitsPastCap() {
        TsCardinalityGuard g = new TsCardinalityGuard(2, TsOverflowPolicy.ALLOW);
        for (int i = 0; i < 5; i++) g.admit();
        assertEquals(5, g.count());
        assertEquals(3, g.overCount());
        assertEquals(0, g.remaining());
        assertTrue(g.wouldExceed());
    }

    @Test
    void guardReleaseFreesASlot() {
        TsCardinalityGuard g = new TsCardinalityGuard(1, TsOverflowPolicy.REJECT);
        g.admit();
        assertThrows(TsCardinalityException.class, g::admit);
        g.release();
        assertEquals(0, g.count());
        g.admit(); // slot reopened
        assertEquals(1, g.count());
    }

    @Test
    void guardReleaseSaturatesAtZero() {
        TsCardinalityGuard g = new TsCardinalityGuard(4, TsOverflowPolicy.REJECT);
        g.release();
        g.release();
        assertEquals(0, g.count());
    }

    @Test
    void guardRemainingAndWouldExceed() {
        TsCardinalityGuard g = new TsCardinalityGuard(2, TsOverflowPolicy.REJECT);
        assertEquals(2, g.remaining());
        assertFalse(g.wouldExceed());
        g.admit();
        assertEquals(1, g.remaining());
        g.admit();
        assertEquals(0, g.remaining());
        assertTrue(g.wouldExceed());
        assertEquals(0, g.overCount()); // reject never over-admits
        assertEquals(2, g.max());
    }

    // ---------- TsTenantedGuard ----------

    @Test
    void tenantedCapsEachTenantIndependently() {
        TsTenantedGuard g = new TsTenantedGuard(2, TsOverflowPolicy.REJECT);
        TsTenantId a = new TsTenantId(1);
        TsTenantId b = new TsTenantId(2);
        g.admit(a);
        g.admit(a);
        TsCardinalityException ex = assertThrows(TsCardinalityException.class, () -> g.admit(a));
        assertEquals(TsCardinalityException.Kind.TENANT_CARDINALITY_CAP, ex.kind());
        assertEquals(1L, ex.tenant());
        // tenant A full does not block tenant B
        g.admit(b);
        g.admit(b);
        assertEquals(2, g.count(a));
        assertEquals(2, g.count(b));
    }

    @Test
    void tenantedCountAndTenants() {
        TsTenantedGuard g = new TsTenantedGuard(5, TsOverflowPolicy.REJECT);
        g.admit(new TsTenantId(10));
        g.admit(new TsTenantId(10));
        g.admit(new TsTenantId(20));
        assertEquals(2, g.count(new TsTenantId(10)));
        assertEquals(1, g.count(new TsTenantId(20)));
        assertEquals(0, g.count(new TsTenantId(99))); // unseen tenant
        List<Long> ids = new ArrayList<>();
        for (TsTenantId t : g.tenants()) ids.add(t.value());
        ids.sort(Long::compareTo);
        assertEquals(List.of(10L, 20L), ids);
        assertEquals(2, g.tenantCount());
    }

    @Test
    void tenantedAllowAndRelease() {
        TsTenantedGuard g = new TsTenantedGuard(1, TsOverflowPolicy.ALLOW);
        TsTenantId t = new TsTenantId(7);
        g.admit(t);
        g.admit(t); // allow climbs past the per-tenant cap
        assertEquals(2, g.count(t));
        assertEquals(0, g.remaining(t));
        g.release(t);
        assertEquals(1, g.count(t));
        assertEquals(1, g.maxPerTenant());
        g.release(new TsTenantId(999)); // release on unseen tenant is a no-op
        assertEquals(1, g.count(t));
    }

    // ---------- TsDedupFilter ----------

    @Test
    void dedupNewThenReplay() {
        TsDedupFilter f = new TsDedupFilter();
        TsIngestKey k = new TsIngestKey(1, 100);
        assertTrue(f.isNew(k));   // first sight
        assertFalse(f.isNew(k));  // replay
        assertFalse(f.isNew(k));
        assertEquals(1, f.seenCount());
        assertTrue(f.contains(k));
    }

    @Test
    void dedupDistinctKeysAreIndependent() {
        TsDedupFilter f = new TsDedupFilter();
        assertTrue(f.isNew(new TsIngestKey(1, 1)));
        assertTrue(f.isNew(new TsIngestKey(1, 2)));  // same series, next seq
        assertTrue(f.isNew(new TsIngestKey(2, 1)));  // diff series, same seq
        assertFalse(f.isNew(new TsIngestKey(1, 1))); // replay of the first
        assertEquals(3, f.seenCount());
    }

    @Test
    void dedupResetClears() {
        TsDedupFilter f = new TsDedupFilter(8);
        assertTrue(f.isEmpty());
        f.isNew(new TsIngestKey(5, 5));
        assertFalse(f.isEmpty());
        f.reset();
        assertEquals(0, f.seenCount());
        assertTrue(f.isEmpty());
        assertTrue(f.isNew(new TsIngestKey(5, 5))); // new again after reset
    }

    // ---------- TsGuardedCollection ----------

    private static TsSeriesMetadata meta(long id, String name) {
        return new TsSeriesMetadata(id, name);
    }

    @Test
    void guardedRegisterUpToCapReturnsIds() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(2, TsOverflowPolicy.REJECT);
        assertEquals(1L, c.register(meta(1, "a")));
        assertEquals(2L, c.register(meta(2, "b")));
        assertEquals(2, c.size());
        assertEquals(0, c.remaining());
    }

    @Test
    void guardedRegisterPastCapThrows() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(1, TsOverflowPolicy.REJECT);
        c.register(meta(1, "a"));
        assertThrows(TsCardinalityException.class, () -> c.register(meta(2, "b")));
        assertEquals(1, c.size());  // no extra series
        assertEquals(1, c.count()); // no consumed slot
    }

    @Test
    void guardedReadsDelegateAndDataIntact() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(8, TsOverflowPolicy.REJECT);
        long id = c.register(meta(42, "cpu"));
        assertTrue(c.push(id, 1, 10.0));
        assertTrue(c.push(id, 2, 20.0));
        TsSeries<Double> s = c.get(id).orElseThrow();
        assertEquals(2, s.size());
        assertEquals(20.0, s.last().orElseThrow().value());
        assertTrue(c.byName("cpu").isPresent());
        assertFalse(c.byName("missing").isPresent());
        assertFalse(c.push(999, 1, 1.0)); // unknown id
    }

    @Test
    void guardedDeregisterFreesASlot() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(1, TsOverflowPolicy.REJECT);
        long id = c.register(meta(1, "a"));
        assertThrows(TsCardinalityException.class, () -> c.register(meta(2, "b")));
        assertTrue(c.deregister(id).isPresent());
        assertTrue(c.isEmpty());
        assertEquals(2L, c.register(meta(2, "b"))); // slot reopened
        assertFalse(c.deregister(404).isPresent());  // unknown id
    }

    @Test
    void guardedDuplicateRegisterReleasesSlot() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(4, TsOverflowPolicy.REJECT);
        c.register(meta(1, "dup"));
        // same id collides inside TsCollection; the guard slot must be returned
        assertThrows(RuntimeException.class, () -> c.register(meta(1, "dup")));
        assertEquals(1, c.count());
        assertEquals(3, c.remaining());
    }

    @Test
    void guardedCollectionAccessorExposesInner() {
        TsGuardedCollection<Double> c = new TsGuardedCollection<>(4, TsOverflowPolicy.ALLOW);
        c.register(meta(1, "a"));
        c.register(meta(2, "b"));
        assertEquals(2, c.collection().size());
    }
}
