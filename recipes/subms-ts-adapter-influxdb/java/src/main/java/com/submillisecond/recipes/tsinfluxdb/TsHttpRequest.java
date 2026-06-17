package com.submillisecond.recipes.tsinfluxdb;

import java.util.List;

/**
 * One HTTP request the adapter hands to a {@link TsInfluxTransport}. {@code
 * path} is path plus query string, e.g.
 * {@code /api/v2/write?org=o&bucket=b&precision=ns}.
 */
public record TsHttpRequest(String method, String path, List<Header> headers, String body) {

    public record Header(String name, String value) {}
}
