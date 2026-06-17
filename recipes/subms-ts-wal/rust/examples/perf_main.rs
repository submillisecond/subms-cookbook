use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_ts_wal::recipe::WalRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&WalRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-wal");
    h.add_meta("subms.recipe.category", "timeseries");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
