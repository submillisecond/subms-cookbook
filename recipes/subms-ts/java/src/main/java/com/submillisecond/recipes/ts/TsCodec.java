package com.submillisecond.recipes.ts;

/**
 * The codec substrate. One value type {@code T} per impl; wrappers (gzip, etc.)
 * delegate to an inner codec. Codec recipes ({@code subms-ts-cbor},
 * {@code subms-ts-gzip}, {@code subms-gorilla-block}) plug in here and compose
 * by wrapping one another.
 */
public interface TsCodec<T> {

    byte[] encode(TsSeries<T> series);

    TsSeries<T> decode(byte[] bytes);

    String format();
}
