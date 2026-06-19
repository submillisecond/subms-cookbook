use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_health::recipe::HealthRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&HealthRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-health");
    h.add_meta("subms.recipe.category", "observability");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
