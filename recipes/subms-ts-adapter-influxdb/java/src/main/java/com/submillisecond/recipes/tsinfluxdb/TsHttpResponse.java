package com.submillisecond.recipes.tsinfluxdb;

/** An HTTP response returned by a {@link TsInfluxTransport}. */
public record TsHttpResponse(int status, String body) {}
