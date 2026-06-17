package com.submillisecond.recipes.mpsc.features;

import java.util.Objects;

/**
 * Thread CPU-affinity helper - best-effort cross-platform mirror of
 * the Rust {@code subms_mpsc_queue::set_affinity}.
 *
 * <p>The JDK does not expose a portable affinity API. Native pinning
 * (Linux {@code sched_setaffinity}, Windows {@code SetThreadAffinityMask})
 * requires JNI / JNA / Project Panama, and pulling in any of those
 * would violate the recipe's zero-runtime-dep rule.
 *
 * <p>This class mirrors the Rust API surface so consumers can wire
 * affinity from either language path, and reports
 * {@link AffinityStatus#UNSUPPORTED} when no implementation is
 * available. The Rust sibling does the real pinning; consumers who
 * need pinned threads on the JVM should reach for a JNA-based
 * helper (e.g. OpenHFT thread-affinity) or arrange OS-level isolation
 * outside the JVM (taskset / numactl on Linux, START /AFFINITY on
 * Windows).
 *
 * <p>Byte-equivalent to the Rust sibling's API shape:
 * {@code set_affinity(&[usize]) -> Result<(), AffinityError>}.
 */
public final class JavaAffinity {

    private JavaAffinity() {}

    /** Result of {@link #setAffinity(int...)}. */
    public enum AffinityStatus {
        /** Pinning succeeded. Not produced by the stock JDK path. */
        OK,
        /** No affinity API on this JVM build. The call was a no-op. */
        UNSUPPORTED,
        /** A core index was out of range or {@code cores} was empty. */
        INVALID_CORE,
    }

    /**
     * Attempt to pin the calling thread to the given logical cores.
     *
     * <p>Returns {@link AffinityStatus#UNSUPPORTED} on the stock JDK
     * with no JNA / Panama wiring (the documented best-effort path).
     * The API surface matches the Rust sibling so feature-gated code
     * can call from either language.
     *
     * @param cores logical CPU indices; empty is rejected.
     * @return result status.
     */
    public static AffinityStatus setAffinity(int... cores) {
        Objects.requireNonNull(cores);
        if (cores.length == 0) {
            return AffinityStatus.INVALID_CORE;
        }
        for (int c : cores) {
            if (c < 0 || c >= 1024) {
                return AffinityStatus.INVALID_CORE;
            }
        }
        // The stock JDK has no portable pinning API; consumers needing
        // pinned threads should layer in JNA / Panama or rely on OS-
        // level isolation outside the JVM. We document the gap rather
        // than pretending to pin.
        return AffinityStatus.UNSUPPORTED;
    }

    /**
     * Convenience overload taking a single core. Equivalent to
     * {@code setAffinity(new int[]{core})}.
     */
    public static AffinityStatus setAffinity(int core) {
        return setAffinity(new int[] {core});
    }

    /**
     * True if the underlying platform / runtime has a real pinning
     * implementation available. Always {@code false} for the stock
     * JDK path; reserved for future JNA / Panama wiring.
     */
    public static boolean isSupported() {
        return false;
    }
}
