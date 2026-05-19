use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_cuckoo_filter::recipe::CuckooFilterRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&CuckooFilterRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
