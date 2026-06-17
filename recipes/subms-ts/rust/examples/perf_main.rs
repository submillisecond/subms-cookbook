use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_ts::recipe::TsRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&TsRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
