#!/usr/bin/env bash
# Writes a markdown rollup to $GITHUB_STEP_SUMMARY summarising the cookbook
# CI matrix. Inputs come from env vars set by the workflow:
#
#   R_RUST     - needs.rust.result        (success | failure | cancelled | skipped)
#   R_JAVA     - needs.java-recipe.result
#   R_PRIMER   - needs.java-primer.result
#   R_CLI      - needs.cli.result
#   C_RECIPES  - needs.changes.outputs.recipes ('true' if recipes/** changed)
#   C_PRIMERS  - needs.changes.outputs.primers
#   C_CLI      - needs.changes.outputs.cli
#
# GITHUB_REPOSITORY + GITHUB_REF_NAME are provided automatically by Actions.
#
# Runs in `set -euo pipefail`; missing required env vars surface immediately.

set -euo pipefail

icon() {
  case "$1" in
    success)   echo ":white_check_mark:" ;;
    failure)   echo ":x:" ;;
    cancelled) echo ":warning:" ;;
    skipped)   echo ":fast_forward:" ;;
    *)         echo ":grey_question:" ;;
  esac
}

: "${R_RUST:?R_RUST not set}"
: "${R_JAVA:?R_JAVA not set}"
: "${R_PRIMER:?R_PRIMER not set}"
: "${R_CLI:?R_CLI not set}"
: "${C_RECIPES:=unknown}"
: "${C_PRIMERS:=unknown}"
: "${C_CLI:=unknown}"
: "${GITHUB_STEP_SUMMARY:?must run under GitHub Actions}"

{
  echo "## subms-cookbook CI"
  echo ""
  echo "| Component | Result | Path filter |"
  echo "|---|---|---|"
  echo "| Rust matrix (16 recipes) | $(icon "$R_RUST") \`$R_RUST\` | \`recipes/**\` = \`$C_RECIPES\` |"
  echo "| Java matrix (16 recipes) | $(icon "$R_JAVA") \`$R_JAVA\` | \`recipes/**\` = \`$C_RECIPES\` |"
  echo "| Java primers (3 primers) | $(icon "$R_PRIMER") \`$R_PRIMER\` | \`primers/**\` = \`$C_PRIMERS\` |"
  echo "| CLI (pnpm test)          | $(icon "$R_CLI") \`$R_CLI\` | \`cli/**\` = \`$C_CLI\` |"
  echo ""

  # Per-recipe coverage table, aggregated from each matrix job's uploaded result
  # (rust tarpaulin line-% + java jacoco line-%, written by {rust,java}-coverage.sh).
  # Present only when the recipe matrix ran (its artifacts landed in CISUM_DIR).
  files=$(ls "${CISUM_DIR:-/nonexistent}"/rust-*.txt "${CISUM_DIR:-/nonexistent}"/java-*.txt 2>/dev/null || true)
  if [ -n "$files" ]; then
    echo "### Per-recipe coverage"
    echo ""
    echo "| Recipe | Rust cov | Rust tests | Java cov | Java tests | Status |"
    echo "|---|--:|--:|--:|--:|:--|"
    recipes=$(printf '%s\n' "$files" | sed -E 's#.*/(rust|java)-(.+)\.txt$#\2#' | sort -u)
    for r in $recipes; do
      rp="-"; rt="-"; rs="n/a"; jp="-"; jt="-"; js="n/a"
      [ -f "$CISUM_DIR/rust-$r.txt" ] && IFS='|' read -r _ rp rt rs < "$CISUM_DIR/rust-$r.txt" || true
      [ -f "$CISUM_DIR/java-$r.txt" ] && IFS='|' read -r _ jp jt js < "$CISUM_DIR/java-$r.txt" || true
      overall=":white_check_mark:"
      case "$rs,$js" in
        *test-fail*) overall=":x: test failed" ;;
        *low-cov*|*error*) overall=":x: below 90%" ;;
      esac
      rc="-"; [ "$rp" != "-" ] && { rc="${rp}%"; [ "$rs" = low-cov ] && rc="$rc :warning:"; }
      jc="-"; [ "$jp" != "-" ] && { jc="${jp}%"; [ "$js" = low-cov ] && jc="$jc :warning:"; }
      echo "| \`$r\` | $rc | $rt | $jc | $jt | $overall |"
    done
    echo ""
  fi

  echo "### Notes"
  echo ""
  echo "- Path-filtered matrix: doc-only or content-only changes skip language jobs entirely."
  echo "- The Rust harness step compiles in release mode (\`cargo test --release --features harness\`); sub_millisecond_bench asserts p99 < 1 ms under the canonical workload."
  echo "- Cross-recipe Java deps (currently subms-lsm-tree -> subms-bloom-filter) get a pre-install step inside the matrix job; other recipes resolve transitively from Maven Central."
  echo "- See [\`recipes/<name>/README.md\`](https://github.com/${GITHUB_REPOSITORY}/tree/${GITHUB_REF_NAME}/recipes) for per-recipe sub-ms claim conditions."
} >> "$GITHUB_STEP_SUMMARY"
