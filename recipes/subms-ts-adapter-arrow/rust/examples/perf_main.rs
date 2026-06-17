use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_ts_arrow::recipe::ArrowRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&ArrowRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-adapter-arrow");
    h.add_meta("subms.recipe.category", "adapter");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
