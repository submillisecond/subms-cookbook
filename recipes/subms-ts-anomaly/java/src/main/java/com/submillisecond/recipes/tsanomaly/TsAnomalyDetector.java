package com.submillisecond.recipes.tsanomaly;

import java.util.ArrayDeque;
import java.util.Optional;

/**
 * A streaming rolling-window z-score anomaly detector. Push points in time
 * order; each value is scored against the mean + standard deviation of the
 * points already in the trailing {@code windowNs}. If the z-score exceeds the
 * threshold, the point is flagged. O(1) amortised per push (running sum +
 * sum-of-squares over a window deque). The live regime-shift / spike detector
 * of the timeseries arc.
 *
 * <pre>
 *   TsAnomalyDetector d = new TsAnomalyDetector(1_000, 3.0); // 1000 ns window, 3 sigma
 *   for (int i = 0; i &lt; 50; i++) d.push(i, 10.0);            // stable baseline
 *   Optional&lt;TsAnomaly&gt; hit = d.push(50, 100.0);             // a 10x spike
 *   // hit.isPresent() &amp;&amp; hit.get().zscore() &gt; 3.0
 * </pre>
 *
 * <p>Scores each value against the window of points strictly before it
 * ({@code latest - ts < windowNs}), then admits it.
 */
public final class TsAnomalyDetector {

    private record Pt(long ts, double value) {
    }

    private final long windowNs;
    private final double sigma;
    private final ArrayDeque<Pt> buf;
    private double sum;
    private double sumSq;

    public TsAnomalyDetector(long windowNs, double sigmaThreshold) {
        this.windowNs = Math.max(1L, windowNs);
        this.sigma = sigmaThreshold;
        this.buf = new ArrayDeque<>();
        this.sum = 0.0;
        this.sumSq = 0.0;
    }

    public int windowCount() {
        return buf.size();
    }

    /**
     * Score {@code value} against the current window, flag it if
     * {@code |z| >= sigma}, then admit it to the window. Returns empty while
     * the window is still warming up (fewer than 2 prior points).
     */
    public Optional<TsAnomaly> push(long ts, double value) {
        long cutoff = ts - windowNs;
        Pt front;
        while ((front = buf.peekFirst()) != null && front.ts() <= cutoff) {
            buf.pollFirst();
            sum -= front.value();
            sumSq -= front.value() * front.value();
        }

        int n = buf.size();
        Optional<TsAnomaly> result = Optional.empty();
        if (n >= 2) {
            double nf = n;
            double mean = sum / nf;
            double var = Math.max(0.0, sumSq / nf - mean * mean);
            double std = Math.max(Math.sqrt(var), 1e-12);
            double z = (value - mean) / std;
            if (Math.abs(z) >= sigma) {
                result = Optional.of(new TsAnomaly(ts, value, z));
            }
        }

        buf.addLast(new Pt(ts, value));
        sum += value;
        sumSq += value * value;
        return result;
    }
}
