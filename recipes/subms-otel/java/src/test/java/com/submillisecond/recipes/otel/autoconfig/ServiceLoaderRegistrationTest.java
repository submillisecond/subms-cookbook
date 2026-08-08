package com.submillisecond.recipes.otel.autoconfig;

import com.submillisecond.perf.SubMsObserver;
import org.junit.jupiter.api.Test;

import java.util.Iterator;
import java.util.ServiceLoader;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

class ServiceLoaderRegistrationTest {

    @Test
    void serviceLoaderDiscoversTestObserver() {
        ServiceLoader<SubMsObserver> loader = ServiceLoader.load(SubMsObserver.class);
        Iterator<SubMsObserver> it = loader.iterator();
        assertTrue(it.hasNext(), "test ServiceLoader entry should be discovered");
        SubMsObserver loaded = it.next();
        assertNotNull(loaded);
        assertTrue(loaded instanceof FakeServiceLoadedObserver,
                "loader should hand back the FakeServiceLoadedObserver test class");
    }

    @Test
    void bootstrapPicksServiceLoadedObserver() {
        SubMsObserver observer = SubMsOtelBootstrap.firstServiceLoadedObserver();
        assertNotNull(observer, "bootstrap helper should find the registered observer");
        assertTrue(observer instanceof FakeServiceLoadedObserver);
    }
}
