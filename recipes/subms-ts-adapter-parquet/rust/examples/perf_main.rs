use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_ts_parquet::recipe::ParquetRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&ParquetRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-adapter-parquet");
    h.add_meta("subms.recipe.category", "adapter");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
