package com.submillisecond.otel.autoconfig;

import com.submillisecond.otel.SubMsOtel;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotNull;

class SubMsOtelAutoConfigTest {

    @Test
    void autoConfigureReturnsAllHandlesWiredUp() {
        SubMsOtelAutoConfig cfg = SubMsOtel.autoConfigure();
        assertNotNull(cfg.meter());
        assertNotNull(cfg.tracer());
        assertNotNull(cfg.observer());
        assertNotNull(cfg.meterProvider());
        assertNotNull(cfg.tracerProvider());
    }

    @Test
    void observerAcceptsRecordWithoutPanicking() {
        SubMsOtelAutoConfig cfg = SubMsOtel.autoConfigure();
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        assertDoesNotThrow(() -> cfg.observer().onRecord(ctx, 1234L));
    }

    @Test
    void providersAcceptForceFlush() {
        SubMsOtelAutoConfig cfg = SubMsOtel.autoConfigure();
        assertDoesNotThrow(() -> cfg.meterProvider().forceFlush().join(1, java.util.concurrent.TimeUnit.SECONDS));
    }
}
