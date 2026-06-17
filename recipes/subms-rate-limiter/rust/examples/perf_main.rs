use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_rate_limiter::recipe::RateLimiterRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&RateLimiterRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-rate-limiter");
    h.add_meta("subms.recipe.category", "scheduling");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
