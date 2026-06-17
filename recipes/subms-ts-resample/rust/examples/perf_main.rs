use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_ts_resample::recipe::ResampleRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&ResampleRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-resample");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
