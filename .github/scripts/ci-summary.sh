#!/usr/bin/env bash
# Writes a markdown rollup to $GITHUB_STEP_SUMMARY summarising the cookbook
# CI matrix. Inputs come from env vars set by the workflow:
#
#   R_RUST     - needs.rust.result        (success | failure | cancelled | skipped)
#   R_JAVA     - needs.java-recipe.result
#   R_GUIDE    - needs.java-guide.result
#   R_CLI      - needs.cli.result
#   C_RECIPES  - needs.changes.outputs.recipes ('true' if recipes/** changed)
#   C_GUIDES   - needs.changes.outputs.guides
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
: "${R_GUIDE:?R_GUIDE not set}"
: "${R_CLI:?R_CLI not set}"
: "${C_RECIPES:=unknown}"
: "${C_GUIDES:=unknown}"
: "${C_CLI:=unknown}"
: "${GITHUB_STEP_SUMMARY:?must run under GitHub Actions}"

{
  echo "## subms-cookbook CI"
  echo ""
  echo "| Component | Result | Path filter |"
  echo "|---|---|---|"
  echo "| Rust matrix (16 recipes) | $(icon "$R_RUST") \`$R_RUST\` | \`recipes/**\` = \`$C_RECIPES\` |"
  echo "| Java matrix (16 recipes) | $(icon "$R_JAVA") \`$R_JAVA\` | \`recipes/**\` = \`$C_RECIPES\` |"
  echo "| Java guides (3 guides)   | $(icon "$R_GUIDE") \`$R_GUIDE\` | \`guides/**\` = \`$C_GUIDES\` |"
  echo "| CLI (pnpm test)          | $(icon "$R_CLI") \`$R_CLI\` | \`cli/**\` = \`$C_CLI\` |"
  echo ""
  echo "### Notes"
  echo ""
  echo "- Path-filtered matrix: doc-only or content-only changes skip language jobs entirely."
  echo "- The Rust harness step compiles in release mode (\`cargo test --release --features harness\`); sub_millisecond_bench asserts p99 < 1 ms under the canonical workload."
  echo "- Cross-recipe Java deps (currently subms-lsm-tree -> subms-bloom-filter) get a pre-install step inside the matrix job; other recipes resolve transitively from Maven Central."
  echo "- See [\`recipes/<name>/README.md\`](https://github.com/${GITHUB_REPOSITORY}/tree/${GITHUB_REF_NAME}/recipes) for per-recipe sub-ms claim conditions."
} >> "$GITHUB_STEP_SUMMARY"
