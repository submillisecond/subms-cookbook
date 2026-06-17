use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_cuckoo_filter::recipe::CuckooFilterRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&CuckooFilterRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-cuckoo-filter");
    h.add_meta("subms.recipe.category", "probabilistic");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
