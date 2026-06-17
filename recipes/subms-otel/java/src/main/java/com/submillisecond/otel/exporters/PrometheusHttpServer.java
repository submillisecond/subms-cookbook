package com.submillisecond.otel.exporters;

import com.sun.net.httpserver.HttpServer;
import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.metrics.InstrumentType;
import io.opentelemetry.sdk.metrics.data.AggregationTemporality;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.MetricExporter;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Collection;

/**
 * Hand-rolled in-process scrape endpoint for the Prometheus text exporter. Owns
 * a JDK {@link HttpServer} listening on the configured port and serves {@code
 * /metrics} from the latest export snapshot. Also implements {@link MetricExporter}
 * so it can be registered directly with a {@code PeriodicMetricReader}.
 *
 * <p>Replaces the {@code opentelemetry-exporter-prometheus} jar.
 */
public final class PrometheusHttpServer implements MetricExporter {

    private final PrometheusTextExporter delegate = new PrometheusTextExporter();
    private final HttpServer server;
    private final int port;

    private PrometheusHttpServer(HttpServer server, int port) {
        this.server = server;
        this.port = port;
        if (server != null) {
            server.createContext("/metrics", exchange -> {
                byte[] body = delegate.scrape().getBytes(StandardCharsets.UTF_8);
                exchange.getResponseHeaders().add("Content-Type", "text/plain; version=0.0.4");
                exchange.sendResponseHeaders(200, body.length);
                try (OutputStream os = exchange.getResponseBody()) {
                    os.write(body);
                }
            });
            server.start();
        }
    }

    public static Builder builder() {
        return new Builder();
    }

    public int getPort() {
        return port;
    }

    public String scrape() {
        return delegate.scrape();
    }

    @Override
    public AggregationTemporality getAggregationTemporality(InstrumentType instrumentType) {
        return AggregationTemporality.CUMULATIVE;
    }

    @Override
    public CompletableResultCode export(Collection<MetricData> metrics) {
        return delegate.export(metrics);
    }

    @Override
    public CompletableResultCode flush() {
        return delegate.flush();
    }

    @Override
    public CompletableResultCode shutdown() {
        if (server != null) {
            server.stop(0);
        }
        return delegate.shutdown();
    }

    public static final class Builder {
        private int port = 9464;
        private String host = "0.0.0.0";

        public Builder setPort(int port) {
            this.port = port;
            return this;
        }

        public Builder setHost(String host) {
            this.host = host;
            return this;
        }

        public PrometheusHttpServer build() {
            try {
                HttpServer s = HttpServer.create(new InetSocketAddress(host, port), 0);
                return new PrometheusHttpServer(s, port);
            } catch (IOException e) {
                throw new IllegalStateException("failed to bind Prometheus HTTP server on " + host + ":" + port, e);
            }
        }
    }
}
