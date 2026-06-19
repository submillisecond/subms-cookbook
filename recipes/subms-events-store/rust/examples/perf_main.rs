use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_events_store::recipe::EventStoreRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&EventStoreRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-events-store");
    h.add_meta("subms.recipe.category", "storage");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
