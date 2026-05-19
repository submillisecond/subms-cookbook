use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_timer_wheel::recipe::TimerWheelRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&TimerWheelRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
