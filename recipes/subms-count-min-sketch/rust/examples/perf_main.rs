use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_count_min_sketch::recipe::CountMinSketchRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&CountMinSketchRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
