//! Zero-dep demo: a checkout saga where the charge fails, rolling back the
//! reservation. Run: `cargo run --example demo`.

use std::sync::{Arc, Mutex};

use subms_events_saga::Saga;

fn main() {
    let undone: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let u1 = Arc::clone(&undone);

    let report = Saga::new("checkout")
        .step(
            "reserve_stock",
            || Ok(()),
            move || {
                u1.lock().unwrap().push("reserve_stock".to_string());
                Ok(())
            },
        )
        .step(
            "charge_card",
            || Err("card declined".to_string()),
            || Ok(()),
        )
        .run();

    println!("outcome: {}", report.outcome.as_str());
    println!("report: {}", report.to_json());
    println!("compensated: {:?}", *undone.lock().unwrap());
}
