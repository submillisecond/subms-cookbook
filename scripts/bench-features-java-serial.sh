#!/usr/bin/env bash
# Serial Java per-feature bench capture, the Java counterpart of
# bench-features-serial.sh. Runs each recipe's PerfFeaturesMain ONE AT A
# TIME (no CPU contention) and writes perf/java-features.json. Validates
# JSON before promoting; a failed run keeps any prior capture.
set -uo pipefail
ROOT="C:/knaier/subms/subms-cookbook/recipes"

run_one() {
  local slug="$1" pkg="$2"
  local jdir="$ROOT/$slug/java"
  local out="$ROOT/$slug/perf/java-features.json"
  [ -f "$jdir/pom.xml" ] || { echo "SKIP  $slug (no java/)"; return; }
  cd "$jdir" || return
  ( mvn -q dependency:build-classpath -Dmdep.outputFile=.cp.txt 2>/dev/null )
  local cp; cp=$(cat .cp.txt 2>/dev/null)
  echo "RUN   $slug  (com.submillisecond.recipes.$pkg.PerfFeaturesMain)"
  java -cp "target/classes;$cp" "com.submillisecond.recipes.$pkg.PerfFeaturesMain" > "$out.tmp" 2>.bench.err
  local rc=$?
  rm -f .cp.txt
  if [ $rc -ne 0 ]; then
    echo "FAIL  $slug (exit $rc); kept previous"; head -4 .bench.err | sed 's/^/      /'; rm -f "$out.tmp" .bench.err; return
  fi
  if python3 -c "import json,sys; d=json.load(open(r'$out.tmp')); sys.exit(0 if d.get('stages') else 1)" 2>/dev/null; then
    mv "$out.tmp" "$out"; echo "OK    $slug -> perf/java-features.json"
  else
    echo "BAD   $slug (invalid JSON); kept previous"; rm -f "$out.tmp"
  fi
  rm -f .bench.err
}

run_one subms-adaptive-radix-tree art
run_one subms-arena-allocator     arena
run_one subms-block-cache         blockcache
run_one subms-bloom-filter        bloom
run_one subms-count-min-sketch    cms
run_one subms-cuckoo-filter       cuckoo
run_one subms-hdr-histogram       hdrhist
run_one subms-hyperloglog         hll
run_one subms-lsm-tree            lsm
run_one subms-merge-iterator      mergeiter
run_one subms-mpsc-queue          mpsc
run_one subms-rate-limiter        ratelimit
run_one subms-segment-reader      segment
run_one subms-spsc-ring-buffer    spsc
run_one subms-timer-wheel         timer
run_one subms-treap               treap
echo "=== serial java-features capture complete ==="
