use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_treap::recipe::TreapRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&TreapRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-treap");
    h.add_meta("subms.recipe.category", "ordered-index");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
