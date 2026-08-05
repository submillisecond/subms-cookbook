#!/usr/bin/env bash
# cargo fmt --check, scoped to the crates that actually have staged changes.
#
# Takes .rs paths as arguments (pre-commit passes the staged set) and resolves
# each to its nearest ancestor Cargo.toml. The cookbook is 24 independent crates
# with no root workspace, so a single `cargo fmt` at the top does nothing, and a
# hardcoded recipe list goes stale the next time one lands. Walking up from the
# file also covers the odd shapes (recipes/subms-otel/example/rust) for free.
#
# No env vars. Run from the repo root.
set -euo pipefail

[ "$#" -gt 0 ] || exit 0

# FIND cargo rather than trusting PATH. Git GUIs launch hooks from a minimal
# environment that usually lacks ~/.cargo/bin, so `command -v cargo` alone means
# the desktop client never runs this gate at all - which is most of the value.
# rustup always installs to $CARGO_HOME/bin (default ~/.cargo/bin), so the
# fallbacks below find it on every machine that has it, including a Windows GUI
# where HOME is unset but USERPROFILE is not.
#
# Skipping is the LAST resort, and only when cargo is genuinely absent. The first
# version of this script had no check at all: `cargo` failed per crate, that was
# read as "unformatted", and it refused the commit while printing `cargo: command
# not found` underneath. Refusing on a measurement that never happened is the
# worst of both.
CARGO="$(command -v cargo 2>/dev/null || true)"
if [ -z "$CARGO" ]; then
  for c in "${CARGO_HOME:-}/bin/cargo" "${HOME:-}/.cargo/bin/cargo" \
           "${USERPROFILE:-}/.cargo/bin/cargo" "${CARGO_HOME:-}/bin/cargo.exe" \
           "${HOME:-}/.cargo/bin/cargo.exe" "${USERPROFILE:-}/.cargo/bin/cargo.exe"; do
    if [ -x "$c" ]; then CARGO="$c"; break; fi
  done
fi
if [ -z "$CARGO" ]; then
  echo "fmt gate skipped: cargo not found on PATH, in CARGO_HOME, or under ~/.cargo (CI still enforces it)" >&2
  exit 0
fi

crates=""
for f in "$@"; do
  d=$(dirname "$f")
  while [ "$d" != "." ] && [ "$d" != "/" ]; do
    if [ -f "$d/Cargo.toml" ]; then
      case " $crates " in
        *" $d "*) ;;
        *) crates="$crates $d" ;;
      esac
      break
    fi
    d=$(dirname "$d")
  done
done

failed=""
for c in $crates; do
  if ! "$CARGO" fmt --manifest-path "$c/Cargo.toml" --check; then
    failed="$failed $c"
  fi
done

if [ -n "$failed" ]; then
  echo "" >&2
  echo "unformatted:$failed" >&2
  echo "fix with: for c in$failed; do cargo fmt --manifest-path \$c/Cargo.toml; done" >&2
  exit 1
fi
