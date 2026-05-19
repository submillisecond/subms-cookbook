use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_hdr_histogram::recipe::HdrHistogramRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&HdrHistogramRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
