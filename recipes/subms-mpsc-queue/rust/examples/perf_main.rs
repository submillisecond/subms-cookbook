use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_mpsc_queue::recipe::MpscQueueRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&MpscQueueRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
