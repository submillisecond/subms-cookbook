#!/usr/bin/env bash
# Run tarpaulin for ONE recipe, record its line-coverage % + test count + status
# for the CI summary table (RUNNER_TEMP/cisum/rust-<recipe>.txt), then enforce the
# >=90% line-coverage gate. Capturing the % ourselves (instead of tarpaulin's
# --fail-under) means the summary shows the number even on a miss.
#
# Env: RECIPE (the recipe slug). Runs in the recipe's rust/ working dir.
set -uo pipefail
: "${RECIPE:?RECIPE not set}"
: "${RUNNER_TEMP:?RUNNER_TEMP not set (run under GitHub Actions)}"

out="$RUNNER_TEMP/tarp-$RECIPE.txt"
# --out Xml additionally writes cobertura.xml, which the CI summary reads for
# the exact covered/missed line counts rather than re-scraping this stdout.
cargo tarpaulin --features full --lib --exclude-files 'src/recipe.rs' --skip-clean --out Xml 2>&1 | tee "$out"

pct=$(grep -oE '[0-9]+\.[0-9]+% coverage' "$out" | tail -1 | grep -oE '^[0-9]+\.[0-9]+' || true)
tests=$(grep -hoE 'test result: ok\. [0-9]+ passed' "$out" | grep -oE '[0-9]+' | awk '{s+=$1} END{print s+0}')

status=pass
if grep -qE 'test result: FAILED|^error(\[|:)' "$out"; then
  status=test-fail
elif [ -z "$pct" ]; then
  status=error
elif awk -v p="$pct" 'BEGIN{ exit (p+0 >= 90 ? 1 : 0) }'; then
  status=low-cov
fi

mkdir -p "$RUNNER_TEMP/cisum"
printf '%s|%s|%s|%s\n' "$RECIPE" "${pct:-0}" "${tests:-0}" "$status" > "$RUNNER_TEMP/cisum/rust-$RECIPE.txt"
echo "rust $RECIPE: ${pct:-?}% line coverage over ${tests:-0} tests -> $status"

[ "$status" = pass ] || { echo "::error::$RECIPE rust coverage gate: $status (${pct:-?}%)"; exit 1; }
