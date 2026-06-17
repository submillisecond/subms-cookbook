package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import java.util.List;

/**
 * InfluxDB v2 adapter. Maps a {@code TsSeriesD} / {@code TsCollection<Double>}
 * to line-protocol writes and decodes Flux annotated-CSV reads, over an
 * injectable {@link TsInfluxTransport}. Behaviour mirrors the Rust sibling; the
 * one documented asymmetry is TLS: the JDK transport speaks https, the Rust
 * std-net transport is plaintext only.
 */
public final class TsInfluxAdapter {

    private final TsInfluxTransport transport;
    private final String org;
    private final String bucket;
    private final String token;

    private TsInfluxAdapter(TsInfluxTransport transport, String token, String org, String bucket) {
        this.transport = transport;
        this.token = token;
        this.org = org;
        this.bucket = bucket;
    }

    /** Connect to an {@code http://} or {@code https://} InfluxDB v2 endpoint. */
    public static TsInfluxAdapter connect(String url, String token, String org, String bucket) {
        if (!url.startsWith("http://") && !url.startsWith("https://")) {
            throw TsInfluxException.config("only http(s):// endpoints are supported");
        }
        return new TsInfluxAdapter(new JdkHttpTransport(url), token, org, bucket);
    }

    /** Build over a caller-supplied transport (the injection point for tests). */
    public static TsInfluxAdapter withTransport(
            TsInfluxTransport transport, String token, String org, String bucket) {
        return new TsInfluxAdapter(transport, token, org, bucket);
    }

    public TsInfluxTransport transport() {
        return transport;
    }

    /** Line-protocol write of one series. Returns the number of points written. */
    public int writeSeries(TsSeriesD series, String measurement) {
        return writeBody(LineProtocol.encodeSeries(series, measurement), series.size());
    }

    /** Write a single generic {@code TsSeries<Double>}. */
    public int writeSeries(TsSeries<Double> series, String measurement) {
        return writeBody(LineProtocol.encodeSeries(series, measurement), series.size());
    }

    /** Line-protocol write of every series in a collection. */
    public int writeCollection(TsCollection<Double> coll) {
        int n = 0;
        for (TsSeries<Double> s : coll.series()) {
            n += s.size();
        }
        return writeBody(LineProtocol.encodeCollection(coll), n);
    }

    private int writeBody(String body, int npoints) {
        if (body.isEmpty()) {
            return 0;
        }
        String path = "/api/v2/write?org=" + pct(org) + "&bucket=" + pct(bucket) + "&precision=ns";
        TsHttpRequest req = new TsHttpRequest(
                "POST",
                path,
                List.of(
                        new TsHttpRequest.Header("Authorization", "Token " + token),
                        new TsHttpRequest.Header("Content-Type", "text/plain; charset=utf-8")),
                body);
        TsHttpResponse resp = transport.send(req);
        if (resp.status() >= 200 && resp.status() < 300) {
            return npoints;
        }
        throw TsInfluxException.http(resp.status(), resp.body());
    }

    /** Run a Flux query and decode the annotated-CSV response into a collection. */
    public TsCollection<Double> queryFlux(String flux) {
        String path = "/api/v2/query?org=" + pct(org);
        TsHttpRequest req = new TsHttpRequest(
                "POST",
                path,
                List.of(
                        new TsHttpRequest.Header("Authorization", "Token " + token),
                        new TsHttpRequest.Header("Content-Type", "application/vnd.flux"),
                        new TsHttpRequest.Header("Accept", "application/csv")),
                flux);
        TsHttpResponse resp = transport.send(req);
        if (resp.status() < 200 || resp.status() >= 300) {
            throw TsInfluxException.http(resp.status(), resp.body());
        }
        return FluxCsv.decodeResponse(resp.body());
    }

    private static String pct(String s) {
        StringBuilder out = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            boolean unreserved = (c >= 'A' && c <= 'Z')
                    || (c >= 'a' && c <= 'z')
                    || (c >= '0' && c <= '9')
                    || c == '-' || c == '_' || c == '.' || c == '~';
            if (unreserved) {
                out.append(c);
            } else {
                out.append(String.format("%%%02X", (int) c));
            }
        }
        return out.toString();
    }
}
