package com.submillisecond.recipes.otel.autoconfig;

import com.submillisecond.recipes.otel.OtelObserver;
import com.submillisecond.recipes.otel.OtelObserverAsync;
import com.submillisecond.recipes.otel.exporters.ExporterOtlpHelper;
import com.submillisecond.recipes.otel.exporters.ExporterPrometheusHelper;
import com.submillisecond.recipes.otel.exporters.ExporterStdoutHelper;
import com.submillisecond.recipes.otel.resource.SubMsOtelResource;
import com.submillisecond.perf.SubMsObserver;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.resources.Resource;
import io.opentelemetry.sdk.trace.SdkTracerProvider;

import java.util.Iterator;
import java.util.ServiceLoader;

/**
 * Env-driven one-line bootstrap. Reads the standard {@code OTEL_*} and {@code SUBMS_*} variables, picks an
 * exporter, builds the resource set, and wires an {@link OtelObserver} / {@link OtelObserverAsync} against
 * the resulting {@link Meter}.
 *
 * <p>{@link ServiceLoader} auto-register: any jar can publish a {@link SubMsObserver} via
 * {@code META-INF/services/com.submillisecond.perf.SubMsObserver}; {@link #autoConfigure()} prefers the first
 * service-loaded observer over building a fresh one.
 */
public final class SubMsOtelBootstrap {

    private SubMsOtelBootstrap() {}

    public static SubMsOtelAutoConfig autoConfigure() {
        Resource resource = SubMsOtelResource.detect();
        ProvidersHolder holder = buildProviders(resource);

        Meter meter = holder.meterProvider.meterBuilder("subms-otel").build();
        Tracer tracer = holder.tracerProvider.tracerBuilder("subms-otel").build();

        SubMsObserver observer = firstServiceLoadedObserver();
        if (observer == null) {
            observer = pickAsync() ? new OtelObserverAsync(meter) : new OtelObserver(meter);
        }
        return new SubMsOtelAutoConfig(meter, tracer, observer, holder.meterProvider, holder.tracerProvider);
    }

    static SubMsObserver firstServiceLoadedObserver() {
        ServiceLoader<SubMsObserver> loader = ServiceLoader.load(SubMsObserver.class);
        Iterator<SubMsObserver> it = loader.iterator();
        if (it.hasNext()) return it.next();
        return null;
    }

    private static boolean pickAsync() {
        String v = System.getenv("SUBMS_OTEL_ASYNC");
        if (v == null) return true;
        v = v.trim().toLowerCase();
        return !(v.equals("false") || v.equals("0") || v.equals("no"));
    }

    static int exemplarK() {
        String v = System.getenv("SUBMS_OTEL_EXEMPLARS_K");
        if (v == null) return 5;
        try {
            return Math.max(1, Integer.parseInt(v.trim()));
        } catch (NumberFormatException e) {
            return 5;
        }
    }

    private static ProvidersHolder buildProviders(Resource resource) {
        String endpoint = System.getenv("OTEL_EXPORTER_OTLP_ENDPOINT");
        boolean haveEndpoint = endpoint != null && !endpoint.isEmpty();

        if (haveEndpoint) {
            try {
                ExporterOtlpHelper.Protocol protocol =
                        ExporterOtlpHelper.Protocol.fromEnv(System.getenv("OTEL_EXPORTER_OTLP_PROTOCOL"));
                ExporterOtlpHelper.Wired wired = ExporterOtlpHelper.build(endpoint, protocol, resource);
                return new ProvidersHolder(wired.meterProvider(), wired.tracerProvider());
            } catch (Throwable ignored) {
                // OTLP build failed; fall through to next backend.
            }
        }

        String promReq = System.getenv("SUBMS_OTEL_PROMETHEUS");
        if (!haveEndpoint && promReq != null && (promReq.equals("1") || promReq.equalsIgnoreCase("true"))) {
            try {
                ExporterPrometheusHelper.Wired wired = ExporterPrometheusHelper.build(resource);
                SdkTracerProvider tp = SdkTracerProvider.builder().setResource(resource).build();
                return new ProvidersHolder(wired.meterProvider(), tp);
            } catch (Throwable ignored) {
                // Prometheus port already bound; fall through.
            }
        }

        try {
            ExporterStdoutHelper.Wired wired = ExporterStdoutHelper.build(resource);
            return new ProvidersHolder(wired.meterProvider(), wired.tracerProvider());
        } catch (Throwable ignored) {
            // No exporter available - hand back empty SDK providers.
            return new ProvidersHolder(
                    SdkMeterProvider.builder().setResource(resource).build(),
                    SdkTracerProvider.builder().setResource(resource).build());
        }
    }

    private record ProvidersHolder(SdkMeterProvider meterProvider, SdkTracerProvider tracerProvider) {}
}
