use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_segment_reader::recipe::SegmentReaderRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&SegmentReaderRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
