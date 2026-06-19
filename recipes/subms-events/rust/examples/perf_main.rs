use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_events::recipe::EventsRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&EventsRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-events");
    h.add_meta("subms.recipe.category", "concurrency");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
