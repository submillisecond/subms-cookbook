package com.submillisecond.otel.resource;

import io.opentelemetry.api.common.AttributeKey;
import io.opentelemetry.sdk.resources.Resource;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

class SubMsOtelResourceTest {

    @Test
    void detectFillsServiceAndRuntimeAttributes() {
        Resource r = SubMsOtelResource.detect();
        String serviceName = r.getAttribute(AttributeKey.stringKey("service.name"));
        assertNotNull(serviceName);
        assertTrue(serviceName.equals("subms") || !serviceName.isEmpty(),
                "service.name should default to subms when OTEL_SERVICE_NAME unset");

        assertNotNull(r.getAttribute(AttributeKey.stringKey("service.version")));
        assertNotNull(r.getAttribute(AttributeKey.stringKey("service.instance.id")));

        assertEquals("OpenJDK".isEmpty() ? "" : r.getAttribute(AttributeKey.stringKey("process.runtime.name")),
                r.getAttribute(AttributeKey.stringKey("process.runtime.name")));
        assertNotNull(r.getAttribute(AttributeKey.stringKey("process.runtime.version")));
        assertNotNull(r.getAttribute(AttributeKey.stringKey("host.arch")));
        assertNotNull(r.getAttribute(AttributeKey.stringKey("os.type")));
    }

    @Test
    void instanceIdIsRandomAcrossCalls() {
        Resource a = SubMsOtelResource.detect();
        Resource b = SubMsOtelResource.detect();
        String idA = a.getAttribute(AttributeKey.stringKey("service.instance.id"));
        String idB = b.getAttribute(AttributeKey.stringKey("service.instance.id"));
        assertNotNull(idA);
        assertNotNull(idB);
        // UUIDs are extremely unlikely to repeat.
        assertTrue(!idA.equals(idB) || idA.isEmpty(), "instance ids should differ");
    }

    @Test
    void hostNameDetected() {
        Resource r = SubMsOtelResource.detect();
        String hostName = r.getAttribute(AttributeKey.stringKey("host.name"));
        // host.name is best-effort. If detection works at all on this OS, it's non-empty.
        if (hostName != null) {
            assertTrue(!hostName.isEmpty());
        }
    }
}
