use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_adaptive_radix_tree::recipe::ArtRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&ArtRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
