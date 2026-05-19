use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_arena_allocator::recipe::ArenaAllocatorRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&ArenaAllocatorRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
