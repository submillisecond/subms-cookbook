use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_segment_reader::recipe::SegmentReaderRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&SegmentReaderRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-segment-reader");
    h.add_meta("subms.recipe.category", "storage");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
