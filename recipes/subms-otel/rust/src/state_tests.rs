use super::*;
use subms_events::EventLevel;

#[test]
fn records_without_a_provider() {
    let r = StateTransitionRecorder::new("subms.test.transition", "test counter");
    r.record("overall", "UP", "DOWN", &[]);
    r.record("db", "UP", "DEGRADED", &[("reason", "timeout".to_string())]);
}

#[test]
fn bridge_forwards_without_a_provider() {
    let b = OtelEventBridge::new();
    assert_eq!(b.name(), "otel");
    b.forward(&Event::transition(
        "subms.health.status",
        EventLevel::Error,
        "db",
        "UP",
        "DOWN",
    ));
    b.forward(&Event::builder("plain").build());
}
