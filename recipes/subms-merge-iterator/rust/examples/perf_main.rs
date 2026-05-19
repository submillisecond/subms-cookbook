use std::io;
use std::process::ExitCode;
use subms::{benchmark, SubMsBenchParams};
use subms_merge_iterator::recipe::MergeIteratorRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&MergeIteratorRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
