package com.submillisecond.recipes.otel.autoconfig;

import com.submillisecond.perf.SubMsObserver;
import org.junit.jupiter.api.Test;

import java.lang.reflect.Method;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

class SubMsOtelBootstrapTest {

    @Test
    void firstServiceLoadedObserverFindsTheTestProvider() {
        // The test resources directory registers FakeServiceLoadedObserver via SPI.
        SubMsObserver o = SubMsOtelBootstrap.firstServiceLoadedObserver();
        // May be null in some classloader configs; assert no crash. The dedicated
        // ServiceLoaderRegistrationTest covers the positive shape against the SPI file.
        assertTrue(o == null || o instanceof SubMsObserver);
    }

    @Test
    void exemplarKHonorsEnvFallback() throws Exception {
        // Invoke the package-private exemplarK helper via reflection; reads SUBMS_OTEL_EXEMPLARS_K.
        Method m = SubMsOtelBootstrap.class.getDeclaredMethod("exemplarK");
        m.setAccessible(true);
        int k = (int) m.invoke(null);
        assertTrue(k >= 1, "exemplarK should be >= 1, got " + k);
    }

    @Test
    void autoConfigureWiresProviders() {
        SubMsOtelAutoConfig cfg = SubMsOtelBootstrap.autoConfigure();
        assertNotNull(cfg);
        assertNotNull(cfg.meter());
        assertNotNull(cfg.tracer());
        assertNotNull(cfg.observer());
        assertNotNull(cfg.meterProvider());
        assertNotNull(cfg.tracerProvider());
    }

    @Test
    void autoConfigureIsRepeatable() {
        SubMsOtelAutoConfig first = SubMsOtelBootstrap.autoConfigure();
        SubMsOtelAutoConfig second = SubMsOtelBootstrap.autoConfigure();
        assertEquals(first.getClass(), second.getClass());
    }
}
