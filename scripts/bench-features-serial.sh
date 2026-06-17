#!/usr/bin/env bash
# Serial per-feature bench capture. Runs each recipe's perf_features
# example ONE AT A TIME so no two benches share the CPU - the parallel
# agent wave produced contention-corrupted p99 numbers (treap base_insert
# read 1.3ms vs a real ~100-500ns). This pass overwrites perf/rust-features.json
# per recipe with trustworthy numbers.
#
# A run writes to a temp file first and only promotes it to
# perf/rust-features.json on a clean exit, so a failed build never
# clobbers an existing good capture.
set -uo pipefail

ROOT="C:/knaier/subms/subms-cookbook/recipes"

# recipe-dir : space-separated opt-in features (harness is always prepended)
run_one() {
  local slug="$1"; shift
  local feats="harness $*"
  local rdir="$ROOT/$slug/rust"
  local out="$ROOT/$slug/perf/rust-features.json"
  local tmp="$rdir/.features-capture.tmp"

  if [ ! -f "$rdir/Cargo.toml" ]; then
    echo "SKIP  $slug (no rust/Cargo.toml)"
    return
  fi
  echo "RUN   $slug  [$feats]"
  ( cd "$rdir" && cargo run --quiet --release --example perf_features --features "$feats" ) > "$tmp" 2> "$rdir/.features-capture.err"
  local rc=$?
  if [ $rc -ne 0 ]; then
    echo "FAIL  $slug (exit $rc) - see $rdir/.features-capture.err; kept previous JSON"
    rm -f "$tmp"
    return
  fi
  # Validate it's JSON with a stages map before promoting.
  if python3 -c "import json,sys; d=json.load(open(r'$tmp')); sys.exit(0 if d.get('stages') else 1)" 2>/dev/null; then
    mv "$tmp" "$out"
    echo "OK    $slug -> perf/rust-features.json"
  else
    echo "BAD   $slug (output not valid JSON with stages); kept previous JSON"
    rm -f "$tmp"
  fi
  rm -f "$rdir/.features-capture.err"
}

run_one subms-bloom-filter        counting scalable partitioned
run_one subms-hyperloglog         sparse union-intersect
run_one subms-count-min-sketch    heavy-hitters windowed merge
run_one subms-cuckoo-filter       variable-fingerprint dynamic concurrent-reads compressed-buckets
run_one subms-adaptive-radix-tree serialize range-scan concurrent-reads metrics compaction
run_one subms-treap               range-query persistent merge-split concurrent-reads
run_one subms-spsc-ring-buffer    bulk wait-strategies mpsc-fan-in mpmc-disruptor metrics
run_one subms-mpsc-queue          mpmc bounded batch metrics
run_one subms-rate-limiter        token-bucket hierarchical distributed-backend metrics
run_one subms-timer-wheel         hierarchical concurrent deadline-scheduler cron metrics
run_one subms-lsm-tree            wal tiered-compaction leveled-compaction snapshot lz4 zstd block-cache-integration
run_one subms-merge-iterator      seek-to tombstones dedup priority
run_one subms-segment-reader      mmap crc32 xxh3 lz4 seek-index wal-cursor
run_one subms-block-cache         arc tinylfu weighted concurrent-shards metrics
run_one subms-arena-allocator     typed growable stats aligned freelist
run_one subms-hdr-histogram       dual-recorder concurrent-writes merge decay value-tagging iterators

echo "=== serial bench-features pass complete ==="
