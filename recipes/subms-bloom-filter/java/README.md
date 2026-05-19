# Bloom filter - Java

OpenJDK 21, Maven. Zero runtime dependencies (JUnit 5 is test-scope
and never ships to consumers). Package: `com.submillisecond.recipes.bloom`.
Standalone reusable recipe; other cookbook samples pick it up as an
ordinary Maven dependency after `mvn install`.

```sh
mvn -q package
mvn -q test                                       # JUnit 5 - correctness + statistical FPR check
mvn -q install                                    # publish to ~/.m2 so other cookbook samples can depend on it
```

## Public API

- `new BloomFilter(int expectedEntries)` - sized at ~10 bits/key, k=7.
- `add(String key)`
- `mightContain(String key) → boolean`
- `writeTo(DataOutputStream)` / `static parse(byte[] buf, int off, int len)`
  - for embedding in a larger file format.

## Consumed by

- [`cookbook/recipes/subms-lsm-tree/java/`](../../subms-lsm-tree/java/) -
  one bloom filter per SSTable, parsed out of the file's trailer.

## Files

- `src/main/java/com/submillisecond/recipes/bloom/BloomFilter.java` -
  implementation. FNV-1a 64-bit produces two 32-bit subhashes for the
  double-hashing trick.
- `src/test/java/com/submillisecond/recipes/bloom/BloomFilterTest.java` -
  JUnit 5: present-key round-trip, serialisation round-trip, empty-filter
  sanity, and a statistical FPR check.
