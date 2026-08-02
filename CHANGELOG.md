# Changelog

All notable changes to the submillisecond.com cookbook (Rust + Java recipes,
primers, and the discovery CLI) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

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
