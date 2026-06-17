use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_tdigest::recipe::TDigestRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&TDigestRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-tdigest");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
