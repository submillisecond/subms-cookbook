//! Sample app: a tour of `subms-events-saga` on a trade-settlement workflow.
//! Run with `cargo run --example sample_app`.
//!
//! The recipe has no optional Cargo features, so the base executor is the whole
//! sample. It tours both saga outcomes over the same four steps
//! (reserve -> match -> settle -> confirm):
//!
//! * commit      - every step succeeds, nothing is rolled back
//! * compensate  - settle fails, so match and reserve unwind in reverse

use std::sync::{Arc, Mutex};

use subms_events_saga::{Event, EventDispatcher, Outcome, Saga, listener};

fn main() {
    commit_path();
    compensate_path();
}

/// A shared ledger the steps mutate: `applied` records forward effects in run
/// order, `undone` records compensations in the order they fired.
#[derive(Default)]
struct Ledger {
    applied: Vec<&'static str>,
    undone: Vec<&'static str>,
}

/// Happy path: reserve margin, match the order, settle the legs, confirm. Every
/// forward succeeds, so the saga commits and no compensation runs.
fn commit_path() {
    println!("== commit: settlement completes end to end ==");
    let ledger = Arc::new(Mutex::new(Ledger::default()));

    let report = build_settlement(&ledger, StepFault::None).run();

    let l = ledger.lock().unwrap();
    println!("  outcome:  {}", report.outcome.as_str());
    println!("  applied:  {:?}", l.applied);
    println!("  json:     {}", report.to_json());
    assert_eq!(report.outcome, Outcome::Committed);
    assert_eq!(l.applied, ["reserve", "match", "settle", "confirm"]);
    assert!(l.undone.is_empty(), "a clean commit rolls nothing back");
}

/// Failure path: settlement of the cash leg fails. The completed steps compensate
/// in reverse (match, then reserve); confirm never ran, so it is not unwound.
fn compensate_path() {
    println!("\n== compensate: settle fails, prior steps unwind in reverse ==");
    let ledger = Arc::new(Mutex::new(Ledger::default()));

    let mut bus = EventDispatcher::sync();
    let phases = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = phases.clone();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock().unwrap().push(format!(
            "{}:{}",
            e.attr("step").unwrap_or(""),
            e.attr("phase").unwrap_or("")
        ));
    }));

    let report = build_settlement(&ledger, StepFault::Settle)
        .with_emitter(bus.handle())
        .run();

    let l = ledger.lock().unwrap();
    println!("  outcome:      {}", report.outcome.as_str());
    println!("  failed_step:  {:?}", report.failed_step.as_deref());
    println!("  applied:      {:?}", l.applied);
    println!("  compensated:  {:?}", report.compensated);
    println!("  undone:       {:?}", l.undone);
    println!("  json:         {}", report.to_json());

    assert_eq!(report.outcome, Outcome::Compensated);
    assert_eq!(report.failed_step.as_deref(), Some("settle"));
    assert_eq!(l.applied, ["reserve", "match"]);
    assert_eq!(report.compensated, ["match", "reserve"]);
    assert_eq!(l.undone, ["match", "reserve"]);

    let phases = phases.lock().unwrap();
    assert!(phases.iter().any(|p| p == "settle:forward_failed"));
    assert!(phases.iter().any(|p| p == "reserve:compensated"));
    println!("  events:       {} lifecycle emissions", phases.len());
}

enum StepFault {
    None,
    Settle,
}

/// Assemble the four-step settlement saga. `fault` decides whether the settle
/// step's forward fails, which is what drives the two outcomes above.
fn build_settlement(ledger: &Arc<Mutex<Ledger>>, fault: StepFault) -> Saga {
    let settle_fails = matches!(fault, StepFault::Settle);
    let mut saga = Saga::new("trade-settlement");
    for name in ["reserve", "match", "settle", "confirm"] {
        let fwd_ledger = ledger.clone();
        let comp_ledger = ledger.clone();
        let forward = move || {
            if name == "settle" && settle_fails {
                return Err("cash leg short".to_string());
            }
            fwd_ledger.lock().unwrap().applied.push(name);
            Ok(())
        };
        let compensate = move || {
            comp_ledger.lock().unwrap().undone.push(name);
            Ok(())
        };
        saga = saga.step(name, forward, compensate);
    }
    saga
}
