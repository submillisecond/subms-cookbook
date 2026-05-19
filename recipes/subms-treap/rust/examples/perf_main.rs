use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_treap::recipe::TreapRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&TreapRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
