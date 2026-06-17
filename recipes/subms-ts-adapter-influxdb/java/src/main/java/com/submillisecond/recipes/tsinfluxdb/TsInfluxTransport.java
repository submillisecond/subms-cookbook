package com.submillisecond.recipes.tsinfluxdb;

/**
 * Pluggable HTTP transport. The adapter builds requests; a transport ships the
 * bytes. The default {@link JdkHttpTransport} uses the JDK's built-in HTTP
 * client; tests inject a {@link CaptureTransport} so request construction and
 * response decoding run without a live server.
 */
public interface TsInfluxTransport {
    TsHttpResponse send(TsHttpRequest req);
}
