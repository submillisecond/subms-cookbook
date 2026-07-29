//! Pins the behaviour the `sample_app` example demonstrates: the settlement saga
//! commits when every step succeeds, and unwinds the completed prefix in reverse
//! when a step fails.

use std::sync::{Arc, Mutex};

use subms_events_saga::{Outcome, Saga};

#[derive(Default)]
struct Ledger {
    applied: Vec<&'static str>,
    undone: Vec<&'static str>,
}

fn build_settlement(ledger: &Arc<Mutex<Ledger>>, settle_fails: bool) -> Saga {
    let mut saga = Saga::new("trade-settlement");
    for name in ["reserve", "match", "settle", "confirm"] {
        let fwd = ledger.clone();
        let comp = ledger.clone();
        let forward = move || {
            if name == "settle" && settle_fails {
                return Err("cash leg short".to_string());
            }
            fwd.lock().unwrap().applied.push(name);
            Ok(())
        };
        let compensate = move || {
            comp.lock().unwrap().undone.push(name);
            Ok(())
        };
        saga = saga.step(name, forward, compensate);
    }
    saga
}

#[test]
fn commit_scenario_applies_every_step() {
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    let report = build_settlement(&ledger, false).run();

    assert_eq!(report.outcome, Outcome::Committed);
    let l = ledger.lock().unwrap();
    assert_eq!(l.applied, ["reserve", "match", "settle", "confirm"]);
    assert!(l.undone.is_empty(), "a clean commit rolls nothing back");
}

#[test]
fn compensate_scenario_unwinds_completed_prefix_in_reverse() {
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    let report = build_settlement(&ledger, true).run();

    assert_eq!(report.outcome, Outcome::Compensated);
    assert_eq!(report.failed_step.as_deref(), Some("settle"));
    assert_eq!(report.compensated, ["match", "reserve"]);

    let l = ledger.lock().unwrap();
    assert_eq!(l.applied, ["reserve", "match"]);
    assert_eq!(l.undone, ["match", "reserve"], "reverse-order rollback");
}
