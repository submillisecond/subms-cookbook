use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_ts_gzip::recipe::GzipRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&GzipRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-gzip");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
