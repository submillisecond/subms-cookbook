#!/usr/bin/env bash
# Publish-readiness check for every Rust crate: build + test (all
# features) + clippy (deny warnings) + fmt --check. Reports one line
# per recipe. Serial to avoid CPU contention skewing nothing (this is
# correctness, not timing) but mainly to keep memory bounded.
set -uo pipefail
ROOT="C:/knaier/subms/subms-cookbook/recipes"

check() {
  local slug="$1"
  local d="$ROOT/$slug/rust"
  [ -f "$d/Cargo.toml" ] || { printf "%-28s SKIP (no rust/)\n" "$slug"; return; }
  cd "$d" || return
  local b t c f
  if cargo build --all-features --quiet 2>/tmp/e_build; then b=ok; else b=FAIL; fi
  if cargo test  --all-features --quiet 2>/tmp/e_test  >/dev/null; then t=ok; else t=FAIL; fi
  if cargo clippy --all-features --all-targets --quiet -- -D warnings 2>/tmp/e_clip; then c=ok; else c=WARN; fi
  if cargo fmt --check --quiet 2>/dev/null; then f=ok; else f=FMT; fi
  printf "%-28s build=%-4s test=%-4s clippy=%-4s fmt=%-4s\n" "$slug" "$b" "$t" "$c" "$f"
  [ "$b" = FAIL ] && { echo "    build err:"; head -4 /tmp/e_build | sed 's/^/      /'; }
  [ "$t" = FAIL ] && { echo "    test err:";  head -4 /tmp/e_test  | sed 's/^/      /'; }
  [ "$c" = WARN ] && { echo "    clippy:";    grep -m3 "warning\|error" /tmp/e_clip | sed 's/^/      /'; }
}

for slug in subms-adaptive-radix-tree subms-arena-allocator subms-block-cache \
  subms-bloom-filter subms-count-min-sketch subms-cuckoo-filter subms-hdr-histogram \
  subms-hyperloglog subms-lsm-tree subms-merge-iterator subms-mpsc-queue \
  subms-rate-limiter subms-segment-reader subms-spsc-ring-buffer subms-timer-wheel subms-treap; do
  check "$slug"
done
echo "=== rust verify complete ==="
