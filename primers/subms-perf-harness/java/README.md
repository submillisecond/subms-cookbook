# subms perf harness primer (Java)

End-to-end walkthrough of the `subms` Java harness against a tiny realistic
workload: timed inserts and lookups into an open-addressing long-keyed map.
Three stages, three p99 assertions, one canonical JSON file. The point of
this project is the harness flow, not the map.

```sh
mvn -q package
mvn -q test                                                                        # JUnit 5 + the embedded sub-ms p99 assertion

# Scratch-paper shape: manual stage, manual time(), one print.
java -cp target/classes com.submillisecond.primers.perfharness.Demo

# Production-style shape: SubMsRecipe + runBench + summarize + assertP99Under.
# stdout is the canonical JSON; stderr is the human-readable table.
echo "entries=20000" | java -cp target/classes \
    com.submillisecond.primers.perfharness.PerfMain > perf/java.json
```

## What this shows

- `SubMsPerfHarness` - record timed samples per named stage, hold the
  inputs / meta sidecar maps.
- `SubMsRecipe` - the wired-up shape: implement two methods, get
  `runBench` + `assertP99Under` + the canonical JSON for free.
- `SubMsBenchParams` - stdin key=value parsing with sensible defaults.
- `SubMsBench.summarize` / `summarizeLean` - typed percentile records.
- `SubMsBench.printSummary` - fixed-width table for the console.
- `SubMsBench.summaryToJson` - the byte-equivalent JSON that the cookbook
  web layer renders (same shape on the Rust side).
- `SubMsBench.assertP99Under` - the gate. Recipes that ship a "sub-ms"
  claim hold the line in this single call.
- `stage.warmThenTime` - the JIT-aware loop that's the difference between
  measuring HotSpot's interpreter and measuring the structure.

## What this is not

- Not a recipe. The structure is contrived and the artifact does not
  publish to Maven Central; this is a primer, read once.
- Not a benchmark of the map. The map is fast enough for the workload to
  measure cleanly; it isn't tuned past that.

## Files

- `src/main/java/com/submillisecond/primers/perfharness/TinyMap.java`
  the small structure under test. Open-addressing, linear probing,
  load-factor 0.75.
- `src/main/java/com/submillisecond/primers/perfharness/HarnessRecipe.java`
  the canonical `SubMsRecipe` wiring with `put` / `get_hit` / `get_miss`
  stages and `warmThenTime` per stage.
- `src/main/java/com/submillisecond/primers/perfharness/PerfMain.java`
  stdin params, runBench, print to stderr, assert p99, emit JSON to stdout.
- `src/main/java/com/submillisecond/primers/perfharness/Demo.java`
  manual harness use without a recipe - the scratch-paper shape.
- `src/test/java/.../TinyMapTest.java` - JUnit 5, 10 tests including a
  HashSet-cross-check fuzz.
- `src/test/java/.../HarnessRecipeTest.java` - JUnit 5, including the
  embedded sub-millisecond p99 assertion against `runBench` output.
