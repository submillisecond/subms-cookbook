#!/usr/bin/env bash
# Record ONE recipe's Java line-coverage % (from the jacoco CSV that `mvn verify`
# already produced) + test count for the CI summary table. Does NOT gate - the
# jacoco 0.90 rule inside `mvn verify` is the gate; this only captures the number
# for the per-recipe table, and runs with `if: always()` so a failed verify still
# reports whatever jacoco managed to write.
#
# Env: RECIPE. Runs in the recipe's java/ working dir.
set -uo pipefail
: "${RECIPE:?RECIPE not set}"
: "${RUNNER_TEMP:?RUNNER_TEMP not set (run under GitHub Actions)}"

csv=$(find target -name jacoco.csv 2>/dev/null | head -1)
pct=0
if [ -n "$csv" ] && [ -f "$csv" ]; then
  # jacoco.csv line columns: LINE_MISSED=$8, LINE_COVERED=$9 (header row skipped).
  pct=$(awk -F, 'NR>1 { m += $8; c += $9 } END { if (m + c > 0) printf "%.2f", 100 * c / (m + c); else print "0" }' "$csv")
fi
tests=$(grep -rhoE 'Tests run: [0-9]+' target/surefire-reports 2>/dev/null | grep -oE '[0-9]+' | awk '{s+=$1} END{print s+0}')

status=pass
awk -v p="${pct:-0}" 'BEGIN{ exit (p+0 >= 90 ? 1 : 0) }' && status=low-cov

mkdir -p "$RUNNER_TEMP/cisum"
printf '%s|%s|%s|%s\n' "$RECIPE" "${pct:-0}" "${tests:-0}" "$status" > "$RUNNER_TEMP/cisum/java-$RECIPE.txt"
echo "java $RECIPE: ${pct:-?}% line coverage over ${tests:-0} tests -> $status"
