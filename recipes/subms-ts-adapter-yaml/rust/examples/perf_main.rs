use std::io;
use std::process::ExitCode;
use subms::{SubMsBenchParams, benchmark};
use subms_ts_yaml::recipe::YamlRecipe;
fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let mut h = benchmark(&YamlRecipe, &params);
    h.add_meta("subms.recipe.slug", "subms-ts-adapter-yaml");
    h.add_meta("subms.recipe.category", "adapter");
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
