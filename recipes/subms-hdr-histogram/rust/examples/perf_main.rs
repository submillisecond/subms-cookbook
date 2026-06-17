use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_hdr_histogram::recipe::HdrHistogramRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&HdrHistogramRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-hdr-histogram");
    h.add_meta("subms.recipe.category", "observability");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
