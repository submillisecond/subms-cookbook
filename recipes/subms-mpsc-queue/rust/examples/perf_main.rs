use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_mpsc_queue::recipe::MpscQueueRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&MpscQueueRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-mpsc-queue");
    h.add_meta("subms.recipe.category", "concurrency");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
