//! Reference-impl divergence counter. Recipes that cross-check their behaviour
//! against a reference library call [`record_reference_divergence`] when the
//! reference output differs from theirs; that fires a single Counter
//! `subms.reference.divergence` with the attribute set the bridge already
//! emits, plus `subms.reference.kind`, `subms.reference.expected`, and
//! `subms.reference.observed`.

use std::sync::Mutex;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Meter};

use subms::SubMsStageKind;

/// Counter name surfaced to OTEL. Stable across versions.
pub const REFERENCE_DIVERGENCE_COUNTER_NAME: &str = "subms.reference.divergence";

/// Attribute key for the reference-impl identity (e.g. `"set_membership"`,
/// `"count_estimate"`).
pub const REFERENCE_KIND_ATTR: &str = "subms.reference.kind";

/// Attribute key for the reference's expected value, stringified.
pub const REFERENCE_EXPECTED_ATTR: &str = "subms.reference.expected";

/// Attribute key for the recipe's observed value, stringified.
pub const REFERENCE_OBSERVED_ATTR: &str = "subms.reference.observed";

/// Build the attribute set carried by every divergence Counter emission.
pub fn divergence_attributes(
    stage: &str,
    kind: SubMsStageKind,
    reference_kind: &str,
    expected: &str,
    observed: &str,
) -> Vec<KeyValue> {
    vec![
        KeyValue::new("subms.stage", stage.to_string()),
        KeyValue::new("subms.stage.kind", kind.as_str()),
        KeyValue::new(REFERENCE_KIND_ATTR, reference_kind.to_string()),
        KeyValue::new(REFERENCE_EXPECTED_ATTR, expected.to_string()),
        KeyValue::new(REFERENCE_OBSERVED_ATTR, observed.to_string()),
    ]
}

/// One-shot helper for recipes that don't want to hold a [`ReferenceDivergenceRecorder`].
/// Builds the counter on the fly; cheap enough for the cross-check call site
/// but allocates a fresh instrument every call - if you call this in a tight
/// loop, build a [`ReferenceDivergenceRecorder`] once and reuse it.
pub fn record_reference_divergence(
    meter: &Meter,
    stage: &str,
    kind: SubMsStageKind,
    reference_kind: &str,
    expected: &str,
    observed: &str,
) {
    let counter: Counter<u64> = meter
        .u64_counter(REFERENCE_DIVERGENCE_COUNTER_NAME)
        .with_description("Count of detected divergences between a recipe and its reference impl")
        .build();
    let attrs = divergence_attributes(stage, kind, reference_kind, expected, observed);
    counter.add(1, &attrs);
}

/// Hold a single Counter instrument so a recipe wiring repeated cross-checks
/// doesn't pay for a fresh `u64_counter` build per divergence. Internally
/// cached; cloning is cheap (the counter is reference-counted in OTEL).
pub struct ReferenceDivergenceRecorder {
    counter: Mutex<Option<Counter<u64>>>,
    meter: Meter,
}

impl ReferenceDivergenceRecorder {
    /// Build bound to the given meter. Counter is built lazily on first record.
    pub fn new(meter: Meter) -> Self {
        Self {
            counter: Mutex::new(None),
            meter,
        }
    }

    /// Record a divergence event. Stage + kind identify the bench surface,
    /// reference_kind names the reference algorithm class, expected + observed
    /// are stringified scalar deltas.
    pub fn record(
        &self,
        stage: &str,
        kind: SubMsStageKind,
        reference_kind: &str,
        expected: &str,
        observed: &str,
    ) {
        let mut slot = self.counter.lock().expect("divergence counter poisoned");
        if slot.is_none() {
            *slot = Some(
                self.meter
                    .u64_counter(REFERENCE_DIVERGENCE_COUNTER_NAME)
                    .with_description(
                        "Count of detected divergences between a recipe and its reference impl",
                    )
                    .build(),
            );
        }
        let counter = slot.as_ref().expect("counter just built");
        let attrs = divergence_attributes(stage, kind, reference_kind, expected, observed);
        counter.add(1, &attrs);
    }
}

#[cfg(test)]
#[path = "drift_tests.rs"]
mod drift_tests;
