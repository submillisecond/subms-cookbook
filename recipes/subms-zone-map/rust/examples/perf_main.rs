use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark};
use subms_zone_map::recipe::ZoneMapRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&ZoneMapRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-zone-map");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
