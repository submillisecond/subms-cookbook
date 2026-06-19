use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_events_saga::recipe::SagaRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&SagaRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-events-saga");
    h.add_meta("subms.recipe.category", "concurrency");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
