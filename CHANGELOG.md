# Changelog

All notable changes to the submillisecond.com cookbook (Rust + Java recipes,
primers, and the discovery CLI) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **All recipes bumped 0.8.2 -> 0.9.0, in lockstep with the harness.** 330
  occurrences across 108 manifests: each recipe's own version, its `subms`
  dependency, and every cross-recipe path dep. 0.9.0 brings the feature-manifest
  v2 classifier - `reported` (a flat op above the 1 ms claim line), `indeterminate`
  (the measurement cannot separate the feature from its guard), per-feature
  provenance, and a per-stage classification API.

  The replace was bounded to `0.8.2` NOT followed by a digit or dot, and verified
  against jacoco's `0.8.13` afterwards. That is not caution for its own sake: an
  unbounded substring replace of a version string corrupted 54 poms earlier in
  this cycle by turning jacoco `0.8.13` into `0.8.23`, and it was caught by an
  offline build rather than by review. Both ports were then compiled against the
  published artifacts - Rust resolved `subms v0.9.0` from crates.io, Java from
  Central - rather than assuming the bump was correct because the text changed.

### Fixed

- **`subms-bloom-filter`: the two ports computed DIFFERENT bit positions.** Java's
  base filter reduced the hash with `Math.floorMod`, Rust with an unsigned `u32`
  remainder. The two agree only when the top bit is clear, so 46% of probe
  positions disagreed (6490 of 14000 measured). A filter written by Rust and
  probed by Java returns FALSE NEGATIVES - the one answer a bloom filter may
  never give - against a page that claimed the structure was "identical across
  Java and Rust".

  It survived because each port passes its OWN tests and `floorMod` also returns
  a valid non-negative index, so nothing looked wrong from either side. The
  `features.*` classes already used `Integer.remainderUnsigned`; only the base
  class was wrong. Now pinned in both suites by a shared hex fixture generated
  from the real code.

  WIRE-FORMAT BEHAVIOUR CHANGE: a filter persisted by an older Java build will
  mis-probe against this one. Needs a version bump and a release note.

- **The Java p99 gate was never run by CI in any prod recipe.**
  `SubmillisecondBench.java` is a `main()`, so surefire never executes it - the
  file looked like a gate while nothing stopped a commit regressing the published
  Java number. Added `SubmillisecondBenchTest` to all four, mirroring each main's
  params and assertions so the two cannot drift. Found independently by two
  reviewers on two different recipes, which is what made it read as corpus-wide
  rather than an oversight in one.

- **`subms-hdr-histogram` published a claim its own capture contradicted.**
  `record p99 < 100 ns` was stated for both ports; the committed Java capture
  says 184 ns. Rewritten per-port with the real figures. Also `~17k counters` in
  five places (diagram, faq, use cases, quality bar) derived from a stale
  `2^9 = 512` sub-bucket comment while the code computes 2048 - real value ~20k.

- **`subms-lsm-tree`: Java was missing the compaction API Rust ships.**
  `compact()`, `set_compaction_trigger()` and `compaction_trigger()` existed in
  Rust and nowhere in Java, while `index.md` claimed the recipe "ships no
  compaction" - wrong in both directions at once. Ported with three mirroring
  tests.

- **`subms-spsc-ring-buffer` was captured under self-contention.** No
  `controls.json`, so a bench that spawns a consumer thread ran under the default
  `cpu_pin: "single"` - both threads time-sharing one isolated core. The ~10 ms
  per-run max on both stages is a scheduler timeslice, not the queue. Added
  `controls.json` with `cpu_pin: multi`; the re-capture must not run without it.
  Its writeup also claimed a "sibling-core producer/consumer" setup that the
  capture never used, plus a 10x padding benchmark and a 3x index-caching win
  that exist in no capture at all.

- **Java `BloomFilter.parse` allocated from an unvalidated header.** A corrupt
  trailer claiming 2^30 words attempted an 8 GB allocation. Rust bounds-checked
  the same field; Java now does too.

- **`subms-mpsc-queue`'s Java feature bench exhausted the heap on `snapshot`.**
  `metrics/snapshot` copies the entire queue, and it was measured with the
  PER-OP rep count (`OPS = 50_000`, warm plus measured) against a `CANON`-sized
  262k-element queue - roughly 100k megabyte-scale arrays, allocated faster than
  the collector could reclaim them. It died with `Java heap space` after the four
  preceding sweeps completed cleanly, which is why it read as a sizing problem;
  no `-Xmx` fits that on a 1 GiB box.

  Same class of error as the `BULK_REPS` bug: a whole-structure op measured with
  a per-op rep count. Snapshot now uses `WHOLE_STRUCTURE_REPS = 512` - still a
  real percentile (`floor(0.99 * n)` leaves samples above it) and small enough
  that the churn fits.

  The RUST port measures the identical thing at the identical rep count and
  survives, because it frees each snapshot immediately and has no garbage to
  outrun. A legitimate runtime difference rather than a parity break, documented
  at the constant - the same shape as the time-boxed-warmup asymmetry already
  recorded for these benches. Verified locally under `-Xmx700m`, the box's own
  ceiling: all five features classify where the run previously died.

- **Six recipes classified bulk ops on the MAXIMUM, not a p99.** The harness
  takes p99 as `sorted[floor(0.99 * n)]`, which at `n <= 100` is exactly
  `n - 1` - the single worst sample. `BULK_REPS` was 30 (adaptive-radix-tree),
  32 (lsm-tree, treap) and 64 (count-min-sketch, hdr-histogram, hyperloglog),
  so every structural-vs-flat verdict on a whole-structure op turned on
  whichever repetition caught a page fault or a scheduler preemption. Only
  `subms-timer-wheel` (256) was computing a real percentile. Raised all six to
  256 in BOTH ports, with the arithmetic written above the constant so it is not
  quietly lowered again.

  Caught because `subms-adaptive-radix-tree`'s `range-scan` and `compaction`
  flipped `structural -> hot-path` on the fleet, which claimed a per-op
  sub-millisecond latency for operations measuring 38.9 ms and 42.5 ms. A full
  unbounded scan cannot be O(1); the curve only read flat because an outlier at
  n=4096 rose to meet n=262144 on a single-core box. The classifier was working
  from a statistic that could not answer the question being asked of it.

  Considered and rejected: feeding the scaling test a median and keeping p99 for
  reporting. Cheaper, and the median is arguably the better estimator for "does
  this grow with n" - but it makes the number the classifier reads differ from
  the number the manifest publishes, and a category whose reason cites a figure
  that appears nowhere else is not auditable. More samples fixes the statistic
  without splitting it. Cost is real: 256 reps of a ~40 ms whole-tree op is ~10 s
  per size per feature, so the heavy recipes take minutes to classify.

- **`subms-arena-allocator` never stamped its Rust feature manifest.** Its
  `perf_features.rs` documented `p99_source: fleet` in the module comment and
  then never called `set_p99_source`, so the file said one thing and the code
  did another. The manifest on disk carries no `p99_source` at all, which reads
  as neither local nor fleet - worse than a wrong stamp, because there is
  nothing to disbelieve. It would also have failed the capture outright:
  `bench-on-fleet.mjs --features` refuses to write a manifest not stamped
  `fleet`, so arena's Rust run would have burned box time and written nothing.
  Swept all 32 generators for the same omission; arena was the only one.
  `subms-adaptive-radix-tree`'s Java manifest is also unstamped, but there the
  generator is correct and the file is merely stale - regenerating fixes it.

- **Java bulk-op warmup must be TIME-BOXED, not a fixed rep count.** Found
  porting `subms-count-min-sketch`. A whole-table op warmed with a fixed 8 reps
  leaves the FIRST sweep point running interpreted, and the sweep shares one
  compiled method across sizes, so every later point is fast: `tick` measured
  608us at width 32768 and 50us at 262144 - eight times the work, an order of
  magnitude faster - a non-monotonic curve that classifies flat, which is the
  cuckoo failure mode arriving by a different route. A budget (300 ms or 5000
  reps, whichever first) reaches C2 at every width: cheap sizes get thousands of
  reps, expensive ones get enough, neither stalls the run. After the fix Java's
  `tick` reads 8800ns at 32768 against Rust's 9200ns, and both ports classify
  structural. Rust needs no equivalent - there is no compilation step to pay -
  so the two ports legitimately differ here and the Java file says why.

- **A genuinely O(n) op can classify FLAT if the sweep starts too small.**
  `classify_feature` wants a time ratio >= 0.5x the size ratio; CMS `tick` over
  the keyed widths (4096..262144) measured 30x over 64x and fell just under it.
  Not noise and not a bug in the classifier: a whole-table clear has a fixed
  per-call cost that dominates an 80 KB table (0.107 ns/cell at 4096 against
  0.051 ns/cell at 262144), and that overhead COMPRESSES the ratio. Fixed by
  sweeping bulk ops an octave higher (32768..2097152), which measures the
  asymptote rather than the call overhead - `tick` then reads 49x/60x over 64x
  and classifies structural in both ports. Recipes now carry two sweep ranges,
  `WIDTHS` for keyed ops and `BULK_WIDTHS` for whole-structure ops. The
  threshold was NOT lowered to make this pass.

- **Rust bulk warmup must be TIME-BOXED too, and not for the JIT reason.** The
  Java fix below is about C2; Rust has no compilation step, and a fixed 8 warm
  reps still was not enough. An op that ALLOCATES has an allocator and
  page-fault ramp: hdr's interval read measured 7000, then 30800, then 44800 ns
  at its smallest sweep point across three runs of unchanged code, flipping the
  feature between structural and hot-path. Both ports now warm to a 300 ms / 5000
  rep budget. Worth stating plainly because the Java lesson invites the wrong
  generalisation - "Java needs warmup, Rust does not" is false for anything that
  touches the allocator.

- **`subms-hdr-histogram`: three O(buckets) ops read flat because the sweep
  started at 32 buckets.** 1/2/3 significant digits gives 32/256/2048
  sub-buckets, and at 32 a fold is entirely fixed per-call cost, so the interval
  read, the decaying record and the percentile walk all classified hot-path.
  Swept over 3/4/5 digits (2048/32768/262144) they read 68x, 83x and clearly
  rising. Same fix as CMS `tick`, arrived at independently - which is the point:
  choosing a sweep range whose SMALL end is already past fixed-cost dominance is
  now a rule, not a one-off.
  - **`decay` is structural, and only because the bench clock moves.** Its
    `record` calls `decay_to_now` first, which brings the whole counter array up
    to date - O(buckets) hiding inside what looks like a single-bucket write.
    With a frozen `ManualClock` the decay pass early-returns and the feature
    measures as a plain record; both ports would have published an O(buckets)
    write as hot-path. Both now inject a clock that advances a millisecond per
    read.
  - **`merge` and the iterators were reading OCCUPANCY, like HLL's union.** They
    visit non-empty buckets, so a fixed 20k recorded values leaves 20k occupied
    whether the array holds 2048 buckets or 262144, and the op stops scaling.
    Filling with `sub_count(d)` values fixes it.
  - **`dual-recorder` is PINNED structural: the op is DESTRUCTIVE and cannot be
    measured by repetition.** The interval read swaps sides and drains, so after
    the first rep each side is empty and every later rep measures nothing.
    Refilling between reps would put the refill inside the timed region, which is
    the bug the ART port shipped. The ports disagree here for exactly that
    reason - Rust copies the counter array unconditionally and reads 468us at
    262144 buckets, Java collapses a high-water index and reads 100ns - and
    neither is the operation a caller performs. Both call the same
    `drain_snapshot`, so this is an implementation asymmetry inside the recipe,
    not a bench difference, and it is worth a follow-up on its own. New trap
    class, distinct from the four already logged: setup-in-the-timed-region,
    statistic mismatch, cold measurement, and occupancy - this one is that the op
    consumes its own input.
  - **`iterators` is PINNED structural.** A percentile walk accumulates across
    the bucket array but emits a BOUNDED ~100 entries at 1% steps however large
    the array, so the per-bucket work amortises better at the top and it measures
    ~57x over a 128x span - under the guard. `iter_linear` was tried as the swept
    op on the reasoning that it visits every bucket; it does not, it steps by
    VALUE unit, so over a 10^7 range it emits millions of entries and never
    finished. Wrong op, not a slower one.

- **`subms-treap` both ports moved to the feature-manifest shape.** Four features
  over three tree sizes. `range-query` and `persistent` read hot-path (O(log n),
  flat by the classifier); `merge-split` and `concurrent-reads` read structural.
  - **`split` is structural, and the reason is not in `split_node`.** The descent
    is O(log n) as advertised, but `split` then calls `count()` on BOTH halves to
    fill in their lengths, and that is a full traversal - an O(log n) op with an
    O(n) bookkeeping tail. Timed as a split-then-merge ROUND TRIP because `split`
    consumes the treap: rebuilding one per rep would put an O(n log n) build
    inside the timed region and the figure would be the build (the ART bug).
  - **The range query is bounded by KEY WIDTH, not by a lazy `take`.** Java's
    `RangeQuery.of` materialises its whole window, so a `take(64)` on a lazy Rust
    iterator has no Java equivalent and the ports would not be measuring the same
    thing. Both now size the window to `(key_space / n) * 64`, which holds the
    result count constant across the sweep - a fixed key width would return 64x
    more rows at the top and the classifier would be reading the answer size.

- **`subms-hyperloglog`: the union sweep was measuring register OCCUPANCY, not
  size.** `estimate_union` allocates an m-register array, folds two more, then
  estimates over m - O(m) three times, not in dispute from the source. It swept
  26x over 64x and classified hot-path. Cause: the key count was held fixed while
  `m` grew, so at p=12 every register is occupied and at p=18 ~92% are zero, and
  `estimate` costs `2^-r` per register where the zero case takes a fast path. The
  sweep read a per-register cost FALLING with size. Filling with `m` keys instead
  of a fixed count holds occupancy constant; it then reads 47.8x (Rust) / 241x
  (Java) and classifies structural in both. General rule this instance teaches:
  when sweeping a structure's size, every other property that feeds the op's cost
  has to be held constant, and fill ratio is the one that hides.

- **`subms-hyperloglog`: `sparse` is PINNED structural in both ports.** Swept over
  precision it is a STEP, not a slope: the promotion threshold is `m/4`, so at
  p=12 and p=15 the structure promotes early and both low points measure the
  DENSE floor rather than a small sparse probe, while at p=18 the list is capped
  by the key count. That read 40x in Rust and 23x in Java - opposite sides of the
  guard, from a measurement artefact rather than a real disagreement. Re-swept
  over sparse-list length with `with_threshold` pinning promotion out of reach,
  a fixed op count, and the list built outside the timed region, it is monotonic
  (1200 / 8400 / 37800 ns) but still only ~25x over 64x, because a long linear
  scan runs ~0.34 ns/element against ~0.93 for a short one. `add` linear-probes
  the list and is O(entries) from the source; hot-path would tell a reader the
  probe is free at high precision. Pinned, with `perfReason` recording that a
  human decided. Second pin in the corpus after cuckoo's `concurrent-reads`.

- **`subms-adaptive-radix-tree`: the compaction bench timed its own setup.** The
  sweep called `bulk_p99(|| { populate(n); delete every other key; compact() })`,
  so the measured "compact" p99 included building an n-key tree and running n/2
  deletes. The setup was inside the closure because `compact()` consumes the
  garbage it compacts and each rep needs a fresh dirty tree - the obvious fix for
  that is the wrong one, because it makes the figure scale with the setup rather
  than with the op. New `bulk_p99_with(setup, op)` helper builds the input
  outside the timed region and times only `op`. Re-run after the fix, compaction
  classifies `structural` (p99 46.7x over a 64x size sweep) with `compact` at
  ~29.7ms - a whole-tree op, correctly excluded from the per-op sub-ms claim.

### Changed

- **`subms-lsm-tree` feature bench swept instead of asserted.**
  `rust/examples/perf_features.rs` and `PerfFeaturesMain` no longer run each
  feature once and stamp `SubMsStageKind::HotPath` on it. Every feature's
  representative op is now swept across three tree sizes (8k / 64k / 512k live
  keys, a 64x span), `classify_feature` / `SubMsFeatureManifest.classify`
  decides the category from the shape of the curve, and the decision plus a
  measured `p99ByStage` is merge-written to `.subms/features/{rust,java}.json`.
  The old shape could not contradict itself: all twelve stages were declared
  hot-path, including a whole-log wal replay that measures 180 ms at 512k
  records.
- **Each feature is swept on the op it exists for, not the cheapest one to
  call.** `wal` is swept on `replay`, not `log_put`; `tiered-compaction` on
  `merge`, not `pick_level`; `leveled-compaction` on `compact`, not
  `pick_level`. The planners' `pick_level` is an O(levels) scan of run counts
  and reads flat at any tree size, so sweeping it would have published "tiered
  compaction is a 200 ns hot-path op" while the merge it schedules runs to
  291 ms. Both planning calls survive in `p99ByStage` as `plan`.

- **Compaction sweeps rebuilt their input outside the timed region.** Both
  compaction entry points CONSUME the level they compact (`mem::take` in Rust,
  `clear()` in Java), so a second rep merges an empty level. A `bulk_each`
  helper rebuilds the manifest before each rep but outside `stage.time`, which
  is what keeps the manifest build out of the number. Rebuilding inside the
  timed closure would have published the setup as the merge; reusing one
  manifest would have published an empty-level no-op as the merge and read
  flat, i.e. classified two O(n) rewrites as hot-path.
- **Per-op warm is time-boxed, not a fixed rep count.** With a fixed 20_000-rep
  warm the block codecs measured 24 ms of warm each, which does not settle an
  allocator that has just had a few hundred megabytes of compaction template
  freed under it. Rust lz4 read 2200 -> 1500 -> 1100 ns across a sweep whose
  axis it does not even touch, and on the previous run of the same binary the
  identical artifact landed on zstd instead (6500 -> 4100 -> 4000). A curve
  that FALLS with N is the exact inverse of the structural signal, and the only
  reason it did not misclassify here is that no plausible drift reaches the
  0.5x-of-size-ratio threshold. A 300 ms / 200k-rep budget (whichever comes
  first, both languages) flattened both: zstd 4000 / 4000 / 4000, lz4 900 / 700
  / 700, with the residual wobble at one to two 100 ns timer ticks.
- **Feature structures now scale with the sweep axis instead of staying
  fixed.** The old bench pinned a 1024-entry block cache, a 50-id snapshot
  manifest and 50 fixed runs regardless of workload. The cache is now sized at
  one 4 KB block per 8 live keys and filled to capacity (constant occupancy,
  not a constant slot count), the snapshot manifest holds one id per 4096 live
  keys, and the compaction manifests hold N entries in 4096-entry runs. Without
  that, a flat curve says nothing: the op was never given a bigger structure to
  be slow on.
- **The compression block is deliberately NOT swept.** lz4 and zstd cost tracks
  the bytes handed to the codec, and an LSM block size is a configuration
  constant, not a function of live key count, so the 4 KB block is held fixed at
  every sweep point. Growing it with N would have published a payload sweep
  dressed as a tree-size sweep and classified both codecs structural.
- **`sync` is absent from the wal stage table on purpose.** fsync is a device
  property, not a code cost - single-digit ms on this laptop tier, tens of us on
  battery-backed NVMe - so a number for it would move with the hardware under a
  column a reader takes as the cost of the code, and sweeping it would dress a
  constant storage-stack cost as a scaling result. `log_put` (buffered append,
  6.5 us p99) and `replay` are what the manifest carries.

### Measured

- Rust: wal structural (85.8x over 64x N), tiered-compaction structural
  (105.4x), leveled-compaction structural (83.4x), snapshot auxiliary (200 ns
  flat vs 2100 ns base get), lz4 auxiliary (900 ns, below base),
  zstd hot-path (4000 ns), block-cache-integration auxiliary (400 ns).
- Java: wal structural (279.8x), tiered-compaction structural (136.3x),
  leveled-compaction structural (83.7x), snapshot auxiliary (100 ns vs 1700 ns
  base), lz4 hot-path (1900 ns), zstd hot-path (3500 ns),
  block-cache-integration auxiliary (100 ns).
- Six of seven categories agree and every stage name matches. The one
  disagreement is `lz4`: Rust measures compress at 900 ns against a 2100 ns
  base get (auxiliary), Java at 1900 ns against a 1700 ns base get, which
  clears the classifier's 10% hot-path margin by 30 ns - two timer ticks. Both
  ports agree on the scaling verdict (flat, not structural); only the
  base-delta side of the call differs, and it is a boundary case, not a bug.

# subms-merge-iterator - perf_features port to the swept feature manifest

- `subms-merge-iterator` perf_features (Rust + Java): replaced the single-size
  bench that ASSERTED `SubMsStageKind::HotPath` / `SubMsStageKind.HOT_PATH` on
  every stage with a sweep over the total element count across the 16 merged
  streams (32768 / 262144 / 2097152, a 64x span) handed to `classify_feature` /
  `SubMsFeatureManifest.classify`. The decision plus a measured `p99ByStage` is
  merge-written to `.subms/features/{rust,java}.json`. All four opt-in features
  (`seek-to`, `tombstones`, `dedup`, `priority`) classify hot-path in both ports,
  which is what the old bench asserted - the difference is that the sweep could
  now have contradicted it.
- `seek` is swept at a FIXED skip distance (64 keys) rather than by spreading a
  fixed number of seeks over the whole key range. The source walks each stream
  forward one entry at a time until it reaches the target - it is a linear skip,
  not a descent or a binary search - so seek cost is set by the skip distance.
  Spreading the seeks would have made the skip distance grow with N and reported
  "seek scales with the input", which is a statement about the bench's own target
  spacing. Same reasoning as the treap recipe holding its range-query result
  count constant.

- A merge step is ~15 ns and this box's clock ticks at 100 ns, so the old bench's
  per-`next()` timing was measuring the clock: 8286 of 20000 single-step samples
  came back as exactly 0 ns and the rest as 100 or 200, which puts the p50 of the
  base merge, dedup, priority and tombstones all at one tick. Every feature would
  have read as a measured non-effect against a base of the same one tick. Fixed
  by timing a BATCH of 64 elements and recording the batch mean, which puts the
  sample at ~1000 ns, ten ticks. The p99 in the manifest is therefore the p99 of
  a 64-element batch mean, not of a single step; unbatched it is unmeasurable
  here.
- The whole-drain trap, avoided by construction and worth stating: an iterator is
  the one shape where timing the op end to end reports the size of the ANSWER.
  64x the elements takes 64x as long at an unchanged per-element cost, so every
  feature would have classified structural. The drain still visits every element
  so the working set scales, but only a fixed 512 batches of it are timed.
- The FIRST measurement a process makes read ~20% high regardless of which point
  it was. Caught by running the sweep largest-first: base then read
  2097152:132 / 262144:104 / 32768:110 ns/element against smallest-first's
  32768:135 / 262144:115 / 2097152:109. Each size reads ~110 when it is not first
  and ~133 when it is. Warming the WORK did not fix it (the untimed warm drain
  was already running 300 ms per measurement); warming the TIMED path did. Both
  ports now run the identical measured closure into a throwaway harness under a
  300 ms budget before the run that counts. Java base afterwards: 100 / 79 / 95,
  flat. The wrong version looked right because a curve that falls with size looks
  like a well-behaved flat feature, and only the reversed run distinguishes
  "first" from "smallest".
- The smallest sweep point was decided by a quarter of the samples of the largest:
  a short input runs out of batches before it runs out of samples (dedup at 32768
  yields 16384 elements = 256 batches against a 512 target), so one scheduling
  blip moved it 30%. Both ports now repeat the drain over a freshly built
  iterator until the sample target is met.
- Java: a measurement's timed region was collecting the PREVIOUS measurement's
  garbage. Every measurement builds a multi-megabyte input and discards it, and
  the seek measurements build the whole n-element input per pass to seek over the
  first 16k of it. The tombstone sweep point at 32768 read 1015 ns/element against
  129 and 121 at the larger sizes - an 8x p50, not a tail. Fixed by collecting
  between the warm and the measured run, and by halving the seek passes (8 -> 4,
  with 2 seeks per timed sample instead of 4) so the bench generates less garbage
  in the first place.

### Ops

- Do not run the two ports concurrently. A Java run made while a Rust run of the
  same recipe was in flight produced the 8x tombstone point above and a base
  curve that moved 40% run to run; run sequentially the same code is stable to
  about 20%.
- `runfeat.sh` leaves a `[[patch.unused]] subms 0.8.2` stanza in the recipe's
  `Cargo.lock` even though it restores `Cargo.toml`. The recipe pins
  `subms = "0.8.1"` and the local unreleased harness is 0.8.2, so the patch does
  not apply until `cargo update -p subms` relocks; the lock then keeps the stanza
  after the toml is restored. Strip it before handing the tree over.

- **A recipe's `Cargo.lock` can silently disable the local-harness patch.** The
  committed lock pins `subms 0.8.1` from the registry while the working tree is
  0.8.2, so appending `[patch.crates-io]` produces `warning: patch ... was not
  used` and a build error on the unreleased API rather than a patched build. The
  lock has to be dropped for the run and restored after; a stale
  `[[patch.unused]]` block left behind in it is the tell.
- **`token-bucket`'s Java curve humps ~1.8x at the middle sweep point**, in every
  run and in either sweep direction (checked by running the sizes descending -
  8192 stayed the slow one, so it is the size and not the position). A fleet of
  8192 buckets plus their per-op BigInteger garbage is the size that neither
  fits in cache nor streams, and is small enough to keep being copied by young
  collections rather than promoted once; a fixed 2g heap did not remove it. It
  does not reach the classifier, which reads the smallest and largest points,
  and Rust's curve is monotone over the same range.

- `subms-segment-reader` `perf_features` (both ports) rewritten to the swept
  feature-manifest shape. Each feature's representative op is now measured
  across three segment lengths (256 / 1024 / 4096 records of a fixed 4 KiB
  block, a 16x span) and `classify_feature` / `SubMsFeatureManifest.classify`
  DECIDES the category from the curve; the decision plus a measured
  `p99ByStage` merge-writes into `.subms/features/{rust,java}.json`. The old
  shape ran every variant at one size and ASSERTED `SubMsStageKind::HotPath`,
  which is an opinion the bench cannot contradict.
- Record COUNT is the sweep axis and the record is pinned at 4 KiB. The
  checksum and compression features cost per byte, so sweeping the payload and
  the segment together would have left every slope with two explanations. The
  4 KiB block is also what lifts a base read clear of the platform timer's
  100 ns tick - at the 12-byte payload `perf_main` uses, a read costs under one
  tick and every feature comparison collapses into quantisation noise.
- Feature stage names now match 1:1 across the ports (`next`, `open`, `seek`,
  `next_after_seek`, `read_committed`, `next_record`).

- **The LZ4 bench was publishing memcpy throughput as its decompression cost.**
  The synthetic block was a random quarter repeated three times: it hits a
  respectable 3.87:1 ratio, and LZ4 encodes it as one literal run plus one 3 KB
  match, so decoding it is two memcpys. Rust read 300 ns for a 4 KiB block -
  about 25 GB/s, which is memory bandwidth, not LZ4. Replaced with a
  pseudo-random sequence of 16-byte tokens drawn from a 64-entry dictionary:
  same ~2.5:1 ratio, but a couple of hundred match/literal steps per block, and
  the number moved to 1700-2300 ns. A compression ratio is not evidence that a
  decoder is doing work, which is exactly why the wrong version looked right.
- **The baseline was measured on the first drain of the process and was
  inflated up to 4x.** The base read is the divisor for every feature category,
  so an inflated one silently demotes real hot-path features to auxiliary - on
  one Java run the base read 1000 ns instead of 300 and `crc32` landed inside
  the classifier's 10% band and published as a measured non-effect. Probing the
  same drain four times in a row gave 1200, 300, 300, 300 ns: the 300 ms warm
  budget inside a single drain does not cover process-level classload, C2
  across the whole `DataInputStream` chain, and the young generation growing to
  absorb 4 KiB per read. Both ports now discard a full first pass and print it
  alongside the kept reading.
- **Per-measurement op count was tied to the sweep point.** Timing exactly one
  drain meant one reader instance, hence one heap placement for its scratch
  buffer, decided the figure: the Rust `xxh3` p50 moved between 400 and 1000 ns
  across runs of unchanged code. Each measurement now repeats drains until it
  has a fixed 65536 samples whatever the segment length, which both spreads the
  figure over several instances and holds the op count constant across sweep
  points.
- **The mmap sweep was a page-cache lottery at the original 4.2 / 16.8 / 67 MB
  span.** With under a gigabyte free on the box, a 67 MB mapped segment does not
  keep its pages between the warm drain and the measured one, so the measured
  pass re-paid faults the warm pass had already paid and the p50 moved 300 ->
  2200 ns between runs. Span reduced 4x to 1.05 / 4.2 / 16.8 MB, which keeps the
  16x ratio and the 4 KiB block while leaving the working set resident. The mmap
  reader is also mapped ONCE per sweep point and rewound between passes rather
  than re-opened, so first-touch faults are paid in the warm loop; re-mapping
  per pass charges every measured read a minor fault and publishes the
  page-fault floor as the read cost.

- Feature categories for this recipe: `crc32`, `xxh3`, `lz4` hot-path in both
  ports; `mmap` and `wal-cursor` auxiliary in both. Both read BELOW the base
  because they hand back a slice of the mapped file / of the caller's buffer,
  while the base reader copies through a `Read` / `DataInputStream`. `seek-index`
  is the one cross-port split - hot-path in Rust (600 ns against a 300 ns base),
  auxiliary in Java (400 ns against a 400 ns base) - and the split is in the
  BASELINE, not the feature: Java's base read allocates and copies twice, so the
  seek does not clear the classifier's 10% band. Both ports agree the curve is
  flat, which is the verdict that matters.
- Nothing in this recipe classifies structural. Every opt-in is a per-record
  decorator on a streaming reader, and the one genuinely O(n) operation - the
  `seek-index` index build at open - is a construction cost paid once per
  reader, not an op a caller repeats. It is deliberately not published as a
  per-op stage, which would read as a latency claim on something nobody calls in
  a loop.

# CHANGELOG rows - subms-spsc-ring-buffer feature bench

- `subms-spsc-ring-buffer` perf_features (both ports) moved from the
  one-size-plus-asserted-`SubMsStageKind::HotPath` shape to the swept feature
  manifest. Each feature's representative op is now swept across three ring
  capacities (1024 / 16384 / 262144 slots, a 256x span that crosses out of L1
  and out of L2), `classify_feature` / `SubMsFeatureManifest.classify` decides
  the category from the curve, and the decision plus a per-stage p99 is
  merge-written to `.subms/features/{rust,java}.json`. The old shape asserted
  hot-path for all twelve stages; the sweep disagrees on three of five features.
- Sweep and p99 now use two different measurement units on purpose. The sweep
  times a sample of 1024 round trips; `p99ByStage` still times one op, matching
  every other recipe's manifest. Single-op timing cannot carry this recipe's
  sweep: a push costs ~3 ns and the dev box's clock ticks at 100 ns, so the old
  bench read p50 = 100 ns and p99 = 200 ns for every stage of every feature -
  base, bulk, disruptor and metrics alike, despite the batched unit later
  showing them 1.6 ns, 3.1 ns, 27 ns and 39 ns apart. Every curve was flat by
  quantisation, so the classifier would have called all five features a measured
  non-effect against an identical base.
- Rings are pre-filled to half capacity and every measured op is a round trip,
  so occupancy is a constant fraction at every sweep point and neither side ever
  takes its full / empty branch. The old bench sized the ring past the whole
  50k run instead, which measures a ring that only ever fills.
- `bulk` is swept on the same ITEM count as every other feature (1024 items,
  moved 32 at a time) rather than on the same CALL count. Sweeping calls would
  have compared a 32-item bulk call against a 1-item push and reported bulk as a
  hot-path cost; per item moved it is 1.6 ns against the base ring's 3.1 ns, and
  classifies auxiliary.
- `wait-strategies` is measured only on a non-full, non-empty ring, so
  `wait()` is never entered. A blocking strategy's wait is a scheduler number -
  `ParkStrategy` sleeps until the far end unparks it - and publishing a park
  latency as the feature's per-op cost would be a category error.
- `mpsc-fan-in` and `mpmc-disruptor` are measured single-threaded at a fixed
  producer / consumer count, isolating the indirection from the contention it
  exists to relieve. `try_publish` scans every consumer cursor, so consumer
  count and not capacity is what that loop is O(); sweeping it would answer a
  different question.

- Sweep points were being drawn from two different CPU clock levels. Every
  measurement on this box lands on one of exactly two values a constant 1.31x
  apart - 3200/4200 for the base ring, 8700/11400 for the fan-in, 27500/36000
  for the disruptor, 39200/51300 for metrics - which is a core-class or
  frequency landing, not anything about the ring. A single pass therefore put
  the LARGEST ring fastest on three of five features, and a median over repeats
  did not help because it mixes the levels: `mpsc-fan-in` read as a clean 1.3x
  rise with size (8700 -> 11400 -> 11500) purely because the small ring drew the
  fast level and the large ones drew the slow one. Fix is seven size-interleaved
  rounds keeping the per-size MINIMUM, which draws every point from the same
  level; the feature then reads flat (8700 / 8700 / 8700). The raw per-round
  values are printed, because the two-level structure is invisible in any
  summary statistic.
- The per-op sink was asymmetric and inverted a result. Blackboxing each pop's
  return forces a 16-byte `Option` through memory on every iteration, and only
  on the features whose pop returns an `Option` - which made the BusySpin
  wrapper measure 20% FASTER than the base ring it wraps (3200 vs 4100 ns per
  sample), an impossibility that read as a legitimate auxiliary classification.
  Both ports now have every op return a `u64` / `long` that the timed loop
  accumulates and sinks once per sample.
- Java pushed `(long) i`, boxing a fresh `Long` per op. That measures the
  allocator on the port that has one and nothing on the port that does not.
  Values now come from a pre-boxed pool cycled by index.
- Java bulk warmup was a fixed 8-ish rep count on a throwaway ring; both ports
  now time-box the warm at 300 ms / 1000 samples. A fixed count leaves the first
  sweep point interpreted (Java) or paying its page-fault ramp (Rust), which
  reads as a curve that FALLS with size - as wrong as a fake rise and harder to
  spot. It is still visible in round 1 of the Java raw rows (a 13-30 us first
  sample against a 6-8 us steady state) and the minimum discards it.

- Run the Java feature bench in its own JVM, not `mvn exec:java`. `exec:java`
  runs the bench inside Maven's JVM, sharing a heap and a GC with the build;
  the identical sweep read 20-35% wider spread per point and turned the
  disruptor and fan-in curves non-monotonic. `java -Xms1g -Xmx1g -cp
  target/classes;<deps>` gives monotonic curves and reproducible categories.
- `runfeat.sh` appends `[patch.crates-io]` but the patch is IGNORED unless
  `cargo update -p subms` runs after it: `Cargo.lock` pins the registry 0.8.1,
  and cargo reports the 0.8.2 path patch as `[[patch.unused]]` in a warning
  that scrolls past the build output. The bench then builds against the
  published harness and fails on the missing `SubMsP99Source`. The patch run
  also writes a `[[patch.unused]]` stanza into `Cargo.lock`, so back up and
  restore the lock alongside `Cargo.toml`.

### Known

- Java `tombstones` sits within noise of the base merge and flips between
  hot-path and auxiliary run to run (base ~95-104 ns/element, tombstones
  ~100-115). Java's base step allocates a heap entry per element and compares
  boxed Longs, so it costs ~100 ns and the tombstone decoration disappears into
  it; Rust's base is ~15 ns and the same decoration reads 31 ns, clearly
  hot-path. Both ports agree on the scaling verdict (flat) at every size. This is
  the accepted near-base disagreement, not a bug to tune away.

- **`subms-mpsc-queue`: a queue op is SMALLER than the platform clock's tick, so
  single-op measurement decided the categories by coin flip.** Ported to the
  swept feature-manifest shape, the first version measured one enqueue/dequeue
  round trip per sample. This box's clock ticks at 100 ns and a round trip costs
  ~35 ns, so every sweep point read exactly 100 or 200 ns and the base-delta test
  was reading which side of a tick boundary an op landed on: `metrics` came back
  `(200, 200, 100)` on one run and `(100, 100, 100)` on the next, flipping
  between hot-path and auxiliary with no code change, and the largest queue read
  FASTER than the smallest. Fixed the way `subms-spsc-ring-buffer` handles the
  same problem - the sweep now times a sample of 1024 round trips, tens of
  microseconds above the tick, while `p99ByStage` still times ONE op so the
  published figures stay comparable across the cookbook. Same run then separates
  the ring variants from the allocating base cleanly and reproducibly.

- **The first sweep point carried the whole process's ramp.** Every measurement
  warms itself, but the sweep runs smallest-first and the FIRST measurement in
  the process pays a ramp its own warm sits inside rather than absorbs: the base
  curve read 71800 / 45300 / 46500 ns, a 1.6x FALL across a 64x size span. That
  is the cold-measurement failure mode arriving one level up from the one the
  per-measurement warm budget already fixes - the budget makes each point
  internally warm, and does nothing about the process being cold when the first
  point runs. Both ports now burn on the base queue (1 s Rust, 2 s Java) before
  the first measurement; the base curve then reads 36900 / 37000 / 37200.

- **Thread migration between core clusters was three times the delta being
  classified.** On this laptop the scheduler moves the bench between core types
  and every measurement lands in one of two clock states 1.31x apart, held for
  longer than a measurement takes. Base and feature could sample different
  states, so `bounded` at a true 0.95x of base could read 1.25x and classify
  hot-path. Two changes: sweep points are now measured INTERLEAVED (all three
  sizes, then all three again, five times) so a slow drift spreads across the
  curve instead of concentrating in one point, and the classified figure is the
  LOWEST of the repeats rather than an average - interference here is one-sided,
  so the minimum compares the queues while a mean compares the boxes. Pinned to
  one core the same sweep repeats to within 1% (`SUBMS_PIN=<core>` on the Rust
  side, via the recipe's own `affinity` feature; the JVM cannot pin itself, so
  the Java port documents `start /affinity` / `taskset` instead). Pinning stays
  OFF by default: a fleet box isolates cores outside the process and pinning from
  inside would override a placement the orchestrator chose.

- **The Java port was measuring `Long` boxing, not the queue.** `push((long) i)`
  boxes a fresh `Long` per enqueue - a second allocation per push that the Rust
  port has no counterpart for - and it handed the sweep to the young collector:
  the base curve read 29000 / 54000 / 33700 ns, a PEAK in the middle, because a
  mid-sized live set is copied between survivor spaces on every young GC while a
  large one is promoted out of the way. Non-monotonic in a direction no queue
  can be. Timed ops now push one pre-boxed element, which is also what a real
  caller does (the object already exists), and the Java base curve reads 23300
  flat across all three sizes.

- **The batch drain's stage was timing the bench's own bookkeeping.** The drain
  measurement nulled out the caller's buffer inside the timed region. In Java
  each of those 256 reference stores carries a card-marking write barrier that
  Rust's `Option<u64>` writes do not, which pushed the Java figure from 25600 to
  27000 ns per 1024 items and flipped `batch` to hot-path there while Rust kept
  it auxiliary - a cross-port category disagreement created entirely by code the
  recipe does not ship. Both ports now time the `try_dequeue_batch` /
  `tryDequeueBatch` call and nothing else, and both classify auxiliary.

- **`subms-mpsc-queue` per-feature bench now MEASURES the category instead of
  asserting it.** Replaced the single-size `SubMsStageKind::HotPath` shape with a
  sweep over resident elements (4096 / 32768 / 262144, a 64x span crossing out of
  L1 and L2) handed to `classify_feature` / `SubMsFeatureManifest.classify`, and
  merge-writes the decision plus a per-stage p99 into
  `.subms/features/{rust,java}.json`. Both ports agree on all five features:
  `bounded` and `batch` auxiliary (Rust measures both BELOW the base round trip -
  the ring avoids the base's per-push allocation, and a 256-wide drain moves an
  item for less than a one-at-a-time pop; Java puts both at 1.099x base, inside
  the classifier's 10% non-effect band by a hair, so those two are the pair a
  different box could legitimately flip), `mpmc` and `metrics` hot-path (1.37x and 1.42x
  base in Rust, 2.16x and 2.35x in Java - the sequence CAS and the two relaxed
  counters are real per-op cost), `affinity` auxiliary. Every curve is flat, as a
  queue's O(1) ops should be; the only measurable slope is `batch` rising 5% over
  the 64x span, which is the cold-node cache effect and nowhere near the O(n)
  threshold.

- **`affinity` is PINNED auxiliary rather than classified.** The bench measures
  it - the curve is printed like every other - but the category is a human call,
  and stated as one in the manifest. `set_affinity` touches no queue state and is
  called once per thread at startup, so it cannot be on the enqueue path at any
  price; left to the base-delta test it would read hot-path in Rust purely
  because a syscall costs more than an enqueue. The two ports are not measuring
  the same thing either: Rust issues a real `SetThreadAffinityMask` /
  `sched_setaffinity` while the stock JDK has no pinning API, so the Java call
  validates its argument and returns UNSUPPORTED. Its Rust cost is also the one
  genuinely unstable number in the bench, moving between ~750 ns and ~2.7 us per
  call across runs - it measures the scheduler as much as the feature, and is
  reported, not claimed.

- **`perf_features` now requires the `affinity` Cargo feature.** The example
  benches all five opt-in features, so its `required-features` list matches
  `full`.

- **`subms-rate-limiter`'s feature bench sweeps instead of asserting.** Both
  ports of `perf_features` / `PerfFeaturesMain` now measure each feature's
  representative op across three sizes and hand the curve to
  `classify_feature` / `SubMsFeatureManifest.classify`, merge-writing the
  decision plus a measured `p99ByStage` into `.subms/features/{rust,java}.json`.
  The old shape ran one size per feature and stamped every stage
  `SubMsStageKind::HotPath`, which is an opinion the bench cannot contradict.
  Categories agree 4/4 across the ports and every stage name matches
  (`try_acquire`, plus `available` for token-bucket and `snapshot` for metrics).
- **The sweep axis is the fleet, not the structure.** A limiter has no internal
  array to grow, so all four features are swept on the number of independent
  limited entities the workload cycles over: buckets for `token-bucket` and
  `metrics`, children for `hierarchical`, live keys for `distributed-backend`.
  `hierarchical` is swept on child count specifically because the source holds
  one parent and a flat `Vec` of children, not a parent chain - a call is an
  index plus three fixed bucket operations, and the sweep is what says so
  rather than a reading of the code.

- **`distributed-backend` was published as hot-path and is structural.** The old
  bench drove `try_acquire` on a single `"hot-key"`, where the backend holds one
  counter and the call looks O(1). `InMemoryBackend::incr` GCs the WHOLE counter
  map on every bump (`counters.retain(...)` in Rust, an entrySet iterator in
  Java), so the per-call cost is O(live keys). Prefilling the backend and
  sweeping 512 -> 8192 live keys reads 1300 -> 35800ns in Rust (27.5x over 16x N)
  and 1500 -> 66800ns in Java (44.5x). One hot key is exactly the shape that
  hides it, which is why the wrong version looked right.
- **The clock was the trap, in both directions.** `TokenBucket::refill_locked`
  early-returns on `elapsed == 0`, so a frozen or manual clock skips the refill
  arithmetic entirely and the feature measures as a compare-and-subtract - in
  Java that arithmetic is BigInteger multiply/divide/add/min, the dominant cost
  of the call. A free-running real clock is the opposite failure: with the
  workload cycling N buckets, real elapsed time between two touches of the SAME
  bucket scales with N, so at 65536 buckets every bucket refills to full and the
  sweep varies token occupancy rather than size. Both ports now inject a
  `SteppingClock` that still READS the platform clock (the production
  `SystemClock` makes that call, and a fixture that skipped it hands every
  feature a saving the base limiter still pays, reading as "cheaper than base")
  but returns a synthetic value stepping a fixed 1us per read, so token accrual
  per call is identical at every sweep point.
- **The accept/reject mix was a function of size, not a constant.** With buckets
  built full, the grant ratio was 55% at 1024 buckets, 88% at 8192 and 100% at
  65536 - at the top each bucket is touched exactly once and a full bucket
  cannot do anything but grant. Setup now drains each bucket into its steady
  state outside the timed region. That alone was not enough: a settled fleet
  drained in LOCKSTEP alternates together and still granted on every first
  touch, so odd-indexed buckets take one extra drain and the mix comes from
  across the fleet. All twelve sweep points now report 50%.
- **Base and `token-bucket` were being compared at the timer's resolution.** Both
  landed on exactly 100ns per op against a 100ns platform tick, and the
  classifier called a mutex-guarded refill a measured non-effect. The
  classification pass now times batches of 16 ops and divides, which puts the
  sample an order of magnitude above the tick and separates them (100 vs 143ns
  Rust, 125 vs 287ns Java) - hot-path in both ports. The published
  `p99ByStage` is still measured one op at a time; a batch mean would hide the
  tail, which is the number the site prints.

### Observation - not changed here, needs a decision

`RingMetrics::max_depth_observed` does not measure depth. `local_depth` is
incremented on every successful push and never decremented - a producer cannot
observe pops - so the gauge reports total items enqueued, and `observe_depth`
takes its compare-exchange on every single push instead of settling once the
high-water mark stops moving. Both ports are identical here, so this is a
semantic question rather than a parity break, and it is what makes `metrics`
cost two read-modify-writes per push rather than one (39 ns per round trip
against the base ring's 3.1 ns in Rust). Fixing it changes published gauge
semantics and the tests that assert on it, so it is left as-is and recorded.

- `subms-timer-wheel` perf_features (both ports) rewritten from the one-size,
  asserted-category shape to the swept feature manifest. Every feature's
  representative op is now measured across three resident-timer counts (32768 /
  131072 / 524288) and `classify_feature` / `SubMsFeatureManifest.classify`
  decides the category from the curve; the decision plus a per-op `p99ByStage`
  merge-writes into `.subms/features/{rust,java}.json`. The old shape asserted
  `SubMsStageKind::HotPath` on all twelve stages, which is an opinion the bench
  had no way to contradict - and it was wrong for three of the five features.
- Swept op per feature is the op the feature INTRODUCES or TRANSFORMS, not the
  cheapest to call: `hierarchical` on `tick`, `deadline-scheduler` on `poll`,
  `cron` on `next_fire`, `concurrent` and `metrics` on `schedule` (they decorate
  the per-op write; sweeping their `tick` would measure the base wheel's bucket
  walk and bill it to a mutex or a counter).

- The tick workload measured the cheap path. The old bench scheduled 50k timers
  over a `SLOTS * 4` horizon and then ticked `SLOTS * 5` times, so most measured
  ticks had nothing due and the hierarchical wheel never cascaded. The new
  workload fires a fixed 1 timer per tick for the whole measured window and puts
  the resident population BEYOND that window, so occupancy is constant and the
  due rate is the same at every sweep point - the sweep reads the wheel, not the
  workload.
- Timer quantum was deciding categories. A single `schedule` costs tens of ns
  and this host's timer quantum is 100 ns, so an unbatched p50 pinned to one or
  two quanta: two runs of unchanged code put `metrics/schedule` at p50 100 and
  p50 300, classifying auxiliary then hot-path. Sweeps now time a batch of 64
  ops per sample (base op batched identically, so the base-delta test still
  compares like with like); `p99ByStage` stays an unbatched per-op figure.
- The baseline was measured once at the top of the run and compared against
  features up to nine seconds and several half-million-timer builds later. On
  this host that gap moved it 3000 -> 4300 ns, as much as a real feature delta.
  It is now re-measured immediately before each feature is classified, and every
  reading is printed so the drift is visible.
- Java keyed measurements were warming the op but not the harness's timed
  wrapper. At batch 64 a measurement enters that wrapper only OPS/64 = 156
  times, nowhere near enough to compile it, so the first keyed measurements of a
  run read 5400 ns against 1600 ns later and `concurrent/schedule` swept
  DOWNWARD across sizes - the under-warm signature, which reads exactly like a
  feature that gets cheaper with more timers. The warm-up now runs through
  `st.time` as well.
- Tick sweep was ratio-compressed at 1024 slots. A tick is a fixed per-call part
  (take the bucket, rebuild the survivors list) plus a per-entry part, and at
  1024 slots the smallest sweep point walked only 32 entries, so the fixed part
  was two thirds of it. Java's `poll` measured exactly 8.5x over 16x - the
  classifier's structural threshold, decided by a rounding, with the Rust port
  reading 10-12x on the same code. Dropping the bench to 256 slots quadruples
  occupancy at the same resident count (the cheap way to start the sweep an
  octave up); `poll` now reads 10-25x in both ports and base `tick` 12-14x.
- `cancel`'s reported p99 was its max. At 32 bulk reps the p99 index IS the
  worst sample, so one OS preemption of a 736 us cancel published 5.2 ms and the
  number moved run to run. Bulk reps raised to 256.
- The deadline scheduler ran on a free-running `MonotonicClock`, so `poll`
  advanced by however many ticks the host happened to take between calls -
  neither repeatable nor comparable across sweep points. Both ports now inject a
  step clock that advances exactly one tick per read.

### Design

- `hierarchical` is PINNED structural, against a flat swept `tick`. The flat
  tick is the correct reading and is the feature's whole point: a cascade moves
  one coarse bucket, which holds the timers due in the next 64 or 4096 ticks, so
  with the due rate fixed the resident population further out costs a tick
  nothing - while the base wheel's own tick walks resident/slots entries on
  every tick (measured 12.8x over a 16x sweep). But the feature also introduces
  `cancel`, which has no id->slot index (the base wheel's index would need
  patching on every cascade) and sweeps all 192 buckets: measured 29-33x over
  16x, 1.0-3.3 ms p99 at 524288 resident. Publishing hot-path off the flat tick
  would tell a reader every op the feature introduces is safe per-operation.
- `metrics` is PINNED auxiliary. `MeteredTimerWheel::schedule` is one non-atomic
  increment of an owned u64 field plus the base call - no allocation, no branch,
  no lock, well under 1% of a ~55 ns schedule. Nothing on this host resolves
  that: the base op's own p50 spreads by a quarter across runs and the feature
  crossed the classifier's 10% band in both directions on four consecutive runs
  of unchanged code. The alternative was shipping a coin toss as a measurement.
- `cron` reads FLAT against resident timers in both ports and that is the
  correct result, not a broken sweep: `next_after` searches forward minute by
  minute from an epoch and never touches a wheel. It still classifies hot-path,
  sitting 2-16x above the base schedule.

- **`subms-cuckoo-filter` both ports moved to the feature-manifest shape.** Four
  features swept across three filter sizes and classified against the base
  filter's lookup. Stage names identical across languages; 3 of 4 categories
  agree (`compressed-buckets` splits auxiliary/hot-path on the same noise floor
  block-cache hit - 100-300ns against a 100ns tick).
  - **`concurrent-reads` is PINNED structural in both ports, not measured, and
    `perfReason` records that.** `CuckooSnapshot::capture` is a `to_vec()` of the
    whole bucket array - unambiguously O(N) from the source - but the sweep will
    not show it on a dev box. Three attempts: a single timed capture read
    277us (cold first-touch landing entirely on the smallest size); 8 warm reps
    brought it to 21us; 16 discarded untimed warmups made no further difference.
    Printing the sweep instead of reasoning about it showed why - `4096 ->
    7100ns, 32768 -> 3000ns, 262144 -> 16300ns`, NON-MONOTONIC, and
    `classify_feature` reads only min and max, so the ratio is 2.3x over a 64x
    size range and the scaling test calls it flat. An O(N) memcpy would have been
    recorded as hot-path. Pinning via the API's `override_category` is the honest
    option: it states a human decided, rather than laundering the decision as
    measured. Revisit on a fleet capture, where the curve should separate.
  - **Third instance of one bug class.** ART's compaction timed its own setup;
    block-cache mixed p50 sweep points with a p99 baseline; this one measures a
    whole-structure op cold at the size that anchors the ratio. Whole-structure
    ops need their setup outside the timed region, their warmup discarded, AND
    their curve inspected - reading the code caught none of the three.

- **`subms-arena-allocator` Java and `subms-block-cache` BOTH ports moved to the
  feature-manifest shape.** Both previously ran every variant at ONE size and
  ASSERTED hot-path via `SubMsStageKind::HOT_PATH`. An asserted category is an
  opinion the bench cannot contradict; each now sweeps three sizes and lets
  `classify_feature` decide, then merge-writes to
  `.subms/features/<lang>.json`. Arena Java mirrors the Rust sweep exactly and
  agrees with it on all five features (typed / growable / stats / aligned /
  freelist, all hot-path) with identical stage names. Block-cache sweeps cache
  capacity and classifies each variant against the base clock-sweep lookup.
  - Two bench bugs found by running them. **Arena Java:** the Rust port sizes a
    fixed-capacity arena to N because Rust needs no warmup; Java's JIT warmup
    means a sweep point performs `warm + N` allocations, so `TypedArena` and
    `AlignedArena` overflowed partway through warmup. Now sized via
    `totalOps(n)`. **Block-cache:** the sweep carries p50s (p99 over a few dozen
    samples is just the worst one and swamps the size signal), but the baseline
    was captured as a p99 - comparing a p50 sweep point against a p99 baseline
    is two different statistics, and the p50 sits under the p99 almost by
    construction, so every feature read as a non-effect. Baseline is now a p50.
  - **Block-cache's hot-path/auxiliary split is NOT trustworthy from a laptop,
    and the two ports say so out loud.** Its per-op costs land at 200-600ns
    against a 100ns timer tick, so the "is this feature above base by >10%"
    test resolves on 1-2 ticks. Running the same sweep in both languages gave
    3 disagreements out of 5 features (arc and concurrent-shards hot-path in
    Java, auxiliary in Rust; metrics the reverse), and a repeat Rust run
    flipped `metrics` unprompted. The SCALING verdicts agreed everywhere and
    every stage name matched, so the ports are structurally in parity - it is
    the near-baseline classification that is reading noise. Deliberately NOT
    fixed by tuning a threshold until the ports agreed: the disagreement is the
    honest reading, and a fleet capture with the full sample count is the
    actual fix. Arena, whose features sit well clear of the noise floor, agreed
    on all five.

- **Feature manifests stamp which box their numbers came from.** Every
  `perf_features` / `PerfFeaturesMain` target now calls
  `set_p99_source(SubMsP99Source::from_env(), ...)` (Rust) /
  `setP99Source(SubMsP99Source.fromEnv(), ...)` (Java), reading
  `SUBMS_FLEET_INSTANCE`. Wired for `subms-bloom-filter` and
  `subms-adaptive-radix-tree`, the two recipes on the manifest API in both
  ports. Requires the unreleased harness API, so these do not compile until
  `subms` publishes - the cross-repo ordering rule (harness first, then
  recipes).
- **`generated_by` in every committed manifest no longer claims the fleet fills
  `p99ByStage`.** It does not and never did; `bench-on-fleet.mjs` has zero
  references to feature manifests. The manifests now read `"p99_source":
  "local"`, which is what they are, and the site will withhold those per-feature
  p99 numbers until a real fleet capture exists.

## [0.7.0] - 2026-07-28

Initial release. `0.7.0` is the baseline version for the cookbook; all earlier
pre-release history is retired and this is the first published version on this
line.
