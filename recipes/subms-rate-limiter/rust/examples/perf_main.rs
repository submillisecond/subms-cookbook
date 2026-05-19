use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_rate_limiter::recipe::RateLimiterRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&RateLimiterRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
