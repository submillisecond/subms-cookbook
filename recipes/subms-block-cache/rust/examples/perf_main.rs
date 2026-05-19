use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_block_cache::recipe::BlockCacheRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&BlockCacheRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
