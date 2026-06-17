package com.submillisecond.recipes.mpsc.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class JavaAffinityTest {

    @Test
    void emptyCoresIsInvalid() {
        assertEquals(JavaAffinity.AffinityStatus.INVALID_CORE, JavaAffinity.setAffinity(new int[0]));
    }

    @Test
    void negativeCoreIsInvalid() {
        assertEquals(JavaAffinity.AffinityStatus.INVALID_CORE, JavaAffinity.setAffinity(-1));
    }

    @Test
    void outOfRangeCoreIsInvalid() {
        assertEquals(JavaAffinity.AffinityStatus.INVALID_CORE, JavaAffinity.setAffinity(2048));
    }

    @Test
    void validCoreReturnsUnsupportedOnStockJdk() {
        // The stock JDK has no portable pinning API; the call documents
        // the gap rather than pretending it succeeded.
        assertEquals(JavaAffinity.AffinityStatus.UNSUPPORTED, JavaAffinity.setAffinity(0));
    }

    @Test
    void multipleCoresReturnsUnsupportedOnStockJdk() {
        assertEquals(JavaAffinity.AffinityStatus.UNSUPPORTED, JavaAffinity.setAffinity(0, 1, 2));
    }

    @Test
    void isSupportedReturnsFalseOnStockJdk() {
        assertFalse(JavaAffinity.isSupported());
    }

    @Test
    void nullArrayRejected() {
        assertThrows(NullPointerException.class, () -> JavaAffinity.setAffinity((int[]) null));
    }

    @Test
    void statusValuesEnumerated() {
        // Sanity: enum surface is what the API documents.
        JavaAffinity.AffinityStatus[] all = JavaAffinity.AffinityStatus.values();
        assertEquals(3, all.length);
        assertNotNull(JavaAffinity.AffinityStatus.valueOf("OK"));
        assertNotNull(JavaAffinity.AffinityStatus.valueOf("UNSUPPORTED"));
        assertNotNull(JavaAffinity.AffinityStatus.valueOf("INVALID_CORE"));
    }
}
