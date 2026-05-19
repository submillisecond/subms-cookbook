use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_hyperloglog::recipe::HyperLogLogRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&HyperLogLogRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
