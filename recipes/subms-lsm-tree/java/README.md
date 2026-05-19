# LSM tree - Java

OpenJDK 21, Maven. Package: `com.submillisecond.recipes.lsm`.

The only dependency is the cookbook's own bloom-filter recipe
([`cookbook/recipes/subms-bloom-filter/java`](../../subms-bloom-filter/java/)).
It must be `mvn install`ed once first so it resolves from `~/.m2`.

```sh
# 1. Install the bloom-filter recipe into the local Maven repo (one-time).
( cd ../../subms-bloom-filter/java && mvn -q install )

# 2. Build this project.
mvn -q package

# 3. Run.
mvn -q test                                                                  # JUnit 5: correctness suite
java -cp target/classes com.submillisecond.recipes.lsm.Demo                  # tiny illustrative scenario
java -cp target/classes com.submillisecond.recipes.lsm.SubMillisecondBench   # perf test (asserts p99 < 1ms)
```

`Demo` and `SubMillisecondBench` are plain `main` entry points -
SubMillisecondBench throws `AssertionError` on a failed perf budget
and exits non-zero.

## Files

- `src/main/java/com/submillisecond/recipes/lsm/Memtable.java` - sorted in-memory
  buffer of pending writes.
- `src/main/java/com/submillisecond/recipes/lsm/SSTable.java` - writes a sorted
  run with a bloom-filter trailer; on open, slurps the whole file into a
  byte buffer and parses the bloom out of the trailer; get short-circuits
  on a bloom miss. Imports `com.submillisecond.recipes.bloom.BloomFilter` from
  the bloom-filter cookbook recipe.
- `src/main/java/com/submillisecond/recipes/lsm/LsmTree.java` - coordinator.
- `src/main/java/com/submillisecond/recipes/lsm/Demo.java` - put / delete /
  flush / get walkthrough on stock symbols.
- `src/test/java/com/submillisecond/recipes/lsm/LsmTreeTest.java` - JUnit 5
  correctness: round-trip, tombstone shadowing, newer-SSTable wins,
  reopen-from-disk, threshold-driven flush, bloom doesn't lose present
  keys.
- `src/main/java/com/submillisecond/recipes/lsm/SubMillisecondBench.java` -
  perf test: 50,000 puts + 50,000 hit-reads + 50,000 miss-reads, prints
  p50/p99/p999/max in microseconds, asserts p99 < 1ms.
