use super::*;

/// The array the growth workload really allocates: values top out just under
/// 1e9, which lands at index 40678, so the array holds 40679 u64 counters.
const WORKLOAD_FOOTPRINT_BYTES: u64 = 325_432;

/// What the harness used to publish - a closed-form 17k-counter estimate that
/// under-reported the allocation by 2.4x.
const OLD_ESTIMATE_BYTES: u64 = 136_000;

fn run_round(recipe: &mut HdrGrowthRecipe, ops: usize) {
    for i in 0..ops {
        recipe.op(0, i);
    }
}

#[test]
fn reports_the_array_the_histogram_actually_holds() {
    let mut recipe = HdrGrowthRecipe::new(3, 2, 5_000);
    run_round(&mut recipe, 5_000);
    let reported = recipe.memory_bytes();
    assert_eq!(reported, recipe.hist.footprint_bytes() as u64);
    assert_eq!(recipe.live_bytes(), reported);
}

#[test]
fn the_workload_footprint_is_the_full_counter_array() {
    let mut recipe = HdrGrowthRecipe::new(3, 1, 50_000);
    run_round(&mut recipe, 50_000);
    assert_eq!(recipe.memory_bytes(), WORKLOAD_FOOTPRINT_BYTES);
    assert_ne!(recipe.memory_bytes(), OLD_ESTIMATE_BYTES);
}

#[test]
fn footprint_is_flat_in_the_sample_count() {
    let mut recipe = HdrGrowthRecipe::new(3, 3, 50_000);
    run_round(&mut recipe, 50_000);
    let after_one = recipe.memory_bytes();
    run_round(&mut recipe, 200_000);
    assert_eq!(recipe.memory_bytes(), after_one);
    assert_eq!(recipe.structures()[0].1, 250_000);
}

#[test]
fn the_bound_covers_the_top_of_the_value_range() {
    let recipe = HdrGrowthRecipe::new(3, 1, 1);
    let (class, bound) = recipe.expected();
    assert!(matches!(class, SubMsGrowthClass::Bounded));
    assert!(
        bound >= WORKLOAD_FOOTPRINT_BYTES as f64,
        "bound {bound} is under the array the workload allocates",
    );
}

#[test]
fn a_wider_precision_needs_a_wider_bound() {
    let (_, three) = HdrGrowthRecipe::new(3, 1, 1).expected();
    let (_, four) = HdrGrowthRecipe::new(4, 1, 1).expected();
    assert!(four > three, "4 significant digits must bound higher");
}

#[test]
fn the_recipe_reports_its_shape() {
    let mut recipe = HdrGrowthRecipe::new(3, 7, 11);
    assert_eq!(recipe.name(), "subms-hdr-histogram");
    assert_eq!(recipe.op_name(), "record");
    assert_eq!(recipe.rounds(), 7);
    assert_eq!(recipe.ops_per_round(), 11);
    assert!(recipe.compact());
    assert_eq!(recipe.structures()[0].0, "records");
}
