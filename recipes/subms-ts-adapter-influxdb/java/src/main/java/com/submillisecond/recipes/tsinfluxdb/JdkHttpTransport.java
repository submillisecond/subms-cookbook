package com.submillisecond.recipes.tsinfluxdb;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;

/**
 * Default transport over the JDK's built-in {@code java.net.http.HttpClient} -
 * no third-party dep, so the adapter stays zero-dep. The base URL is plaintext
 * or TLS as the JDK client supports; the Rust sibling's std-net transport is
 * plaintext only, which is the documented floor for cross-language parity.
 * Excluded from coverage: it is the live-network boundary, not unit-testable
 * library code.
 */
public final class JdkHttpTransport implements TsInfluxTransport {

    private final HttpClient client;
    private final String base;

    public JdkHttpTransport(String base) {
        this.base = base.endsWith("/") ? base.substring(0, base.length() - 1) : base;
        this.client = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(10)).build();
    }

    @Override
    public TsHttpResponse send(TsHttpRequest req) {
        try {
            HttpRequest.Builder b = HttpRequest.newBuilder()
                    .uri(URI.create(base + req.path()))
                    .method(
                            req.method(),
                            HttpRequest.BodyPublishers.ofString(req.body()));
            for (TsHttpRequest.Header h : req.headers()) {
                b.header(h.name(), h.value());
            }
            HttpResponse<String> resp =
                    client.send(b.build(), HttpResponse.BodyHandlers.ofString());
            return new TsHttpResponse(resp.statusCode(), resp.body());
        } catch (java.io.IOException e) {
            throw TsInfluxException.transport(e.getMessage());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw TsInfluxException.transport("interrupted");
        }
    }
}
