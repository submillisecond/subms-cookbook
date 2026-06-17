use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_ts_influxdb::recipe::InfluxRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&InfluxRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-adapter-influxdb");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
