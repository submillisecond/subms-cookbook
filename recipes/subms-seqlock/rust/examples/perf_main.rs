//! Run the seqlock perf workload via the standard `subms` harness.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example perf_main --features harness
//! entries=1000000
//! warmup=10000
//! seed=0
//! EOF
//! ```

use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_seqlock::recipe::SeqLockRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&SeqLockRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-seqlock");
    h.add_meta("subms.recipe.category", "concurrency");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
