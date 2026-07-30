use std::sync::{Arc, Mutex};

use super::*;

fn ok() -> Result<(), String> {
    Ok(())
}

#[test]
fn commit_all_succeed() {
    let r = Saga::new("x").step("a", ok, ok).step("b", ok, ok).run();
    assert_eq!(r.outcome, Outcome::Committed);
    assert!(r.is_committed());
    assert_eq!(r.forward_ran, ["a", "b"]);
    assert!(r.compensated.is_empty());
    assert!(r.failed_step.is_none());
}

#[test]
fn compensates_on_failure_in_reverse() {
    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (o1, o2) = (Arc::clone(&order), Arc::clone(&order));
    let r = Saga::new("x")
        .step("a", ok, move || {
            o1.lock().unwrap().push("a".into());
            Ok(())
        })
        .step("b", ok, move || {
            o2.lock().unwrap().push("b".into());
            Ok(())
        })
        .step("c", || Err("boom".into()), ok)
        .run();
    assert_eq!(r.outcome, Outcome::Compensated);
    assert_eq!(r.failed_step.as_deref(), Some("c"));
    assert_eq!(r.reason.as_deref(), Some("boom"));
    assert_eq!(r.forward_ran, ["a", "b"]);
    assert_eq!(r.compensated, ["b", "a"]); // reverse order
    assert_eq!(*order.lock().unwrap(), ["b", "a"]); // compensations actually ran, reversed
}

#[test]
fn first_step_failure_compensates_nothing() {
    let r = Saga::new("x")
        .step("a", || Err("no".into()), ok)
        .step("b", ok, ok)
        .run();
    assert_eq!(r.outcome, Outcome::Compensated);
    assert_eq!(r.failed_step.as_deref(), Some("a"));
    assert!(r.forward_ran.is_empty());
    assert!(r.compensated.is_empty());
}

#[test]
fn middle_step_failure_compensates_earlier_only() {
    let r = Saga::new("x")
        .step("a", ok, ok)
        .step("b", || Err("no".into()), ok)
        .step("c", ok, ok)
        .run();
    assert_eq!(r.forward_ran, ["a"]);
    assert_eq!(r.compensated, ["a"]);
    assert_eq!(r.failed_step.as_deref(), Some("b"));
}

#[test]
fn compensation_failure_is_recorded() {
    let r = Saga::new("x")
        .step("a", ok, || Err("rollback failed".into()))
        .step("b", || Err("boom".into()), ok)
        .run();
    assert_eq!(r.outcome, Outcome::Compensated);
    assert!(r.compensated.is_empty());
    assert_eq!(
        r.compensation_failures,
        [("a".to_string(), "rollback failed".to_string())]
    );
}

#[test]
fn empty_saga_commits() {
    let r = Saga::new("x").run();
    assert_eq!(r.outcome, Outcome::Committed);
    assert!(r.forward_ran.is_empty());
    assert_eq!(
        r.to_json(),
        "{\"outcome\":\"COMMITTED\",\"forward_ran\":[]}"
    );
}

#[test]
fn forward_actions_run() {
    let hits = Arc::new(Mutex::new(0));
    let (h1, h2) = (Arc::clone(&hits), Arc::clone(&hits));
    Saga::new("x")
        .step(
            "a",
            move || {
                *h1.lock().unwrap() += 1;
                Ok(())
            },
            ok,
        )
        .step(
            "b",
            move || {
                *h2.lock().unwrap() += 1;
                Ok(())
            },
            ok,
        )
        .run();
    assert_eq!(*hits.lock().unwrap(), 2);
}

#[test]
fn outcome_tokens() {
    assert_eq!(Outcome::Committed.as_str(), "COMMITTED");
    assert_eq!(Outcome::Compensated.as_str(), "COMPENSATED");
}

#[test]
fn emits_step_lifecycle_events() {
    let phases: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&phases);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock().unwrap().push(format!(
            "{}:{}",
            e.attr("step").unwrap_or(""),
            e.attr("phase").unwrap_or("")
        ))
    }));
    Saga::new("x")
        .with_emitter(bus.handle())
        .step("a", ok, ok)
        .step("b", || Err("boom".into()), ok)
        .run();
    let got = phases.lock().unwrap().clone();
    assert!(got.contains(&"a:forward_completed".to_string()));
    assert!(got.contains(&"b:forward_failed".to_string()));
    assert!(got.contains(&"a:compensated".to_string()));
}

#[test]
fn emits_committed_event_on_success() {
    let phases: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&phases);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock()
            .unwrap()
            .push(e.attr("phase").unwrap_or("").to_string())
    }));
    Saga::new("x")
        .with_emitter(bus.handle())
        .step("a", ok, ok)
        .run();
    assert!(phases.lock().unwrap().contains(&"committed".to_string()));
}

#[test]
fn emits_compensation_failed_event() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock().unwrap().push(format!(
            "{}:{}:{}",
            e.attr("step").unwrap_or(""),
            e.attr("phase").unwrap_or(""),
            e.message.as_deref().unwrap_or("")
        ))
    }));
    Saga::new("x")
        .with_emitter(bus.handle())
        .step("a", ok, || Err("rollback failed".into()))
        .step("b", || Err("boom".into()), ok)
        .run();
    let got = seen.lock().unwrap().clone();
    assert!(got.contains(&"a:compensation_failed:rollback failed".to_string()));
}

#[test]
fn cross_language_committed_fixture() {
    let r = Saga::new("x").step("a", ok, ok).step("b", ok, ok).run();
    assert_eq!(
        r.to_json(),
        "{\"outcome\":\"COMMITTED\",\"forward_ran\":[\"a\",\"b\"]}"
    );
}

#[test]
fn cross_language_compensated_fixture() {
    let r = Saga::new("x")
        .step("a", ok, ok)
        .step("b", ok, ok)
        .step("c", || Err("boom".into()), ok)
        .run();
    assert_eq!(
        r.to_json(),
        "{\"outcome\":\"COMPENSATED\",\"failed_step\":\"c\",\"reason\":\"boom\",\"forward_ran\":[\"a\",\"b\"],\"compensated\":[\"b\",\"a\"],\"compensation_failures\":[]}"
    );
}

#[test]
fn json_escaping_in_reason() {
    let r = Saga::new("x").step("a", || Err("a\"b\\c".into()), ok).run();
    assert!(r.to_json().contains("\\\"b\\\\c"));
}
