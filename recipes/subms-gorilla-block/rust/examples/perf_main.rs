use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_gorilla_block::recipe::GorillaRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&GorillaRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-gorilla-block");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
