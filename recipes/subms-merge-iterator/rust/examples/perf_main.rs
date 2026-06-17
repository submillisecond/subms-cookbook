use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_merge_iterator::recipe::MergeIteratorRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&MergeIteratorRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-merge-iterator");
    h.add_meta("subms.recipe.category", "storage");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
