use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_block_cache::recipe::BlockCacheRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&BlockCacheRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-block-cache");
    h.add_meta("subms.recipe.category", "memory");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
