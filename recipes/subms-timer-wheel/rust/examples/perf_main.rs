use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_timer_wheel::recipe::TimerWheelRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&TimerWheelRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-timer-wheel");
    h.add_meta("subms.recipe.category", "scheduling");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
