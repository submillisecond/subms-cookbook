use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_adaptive_radix_tree::recipe::ArtRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&ArtRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-adaptive-radix-tree");
    h.add_meta("subms.recipe.category", "ordered-index");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
