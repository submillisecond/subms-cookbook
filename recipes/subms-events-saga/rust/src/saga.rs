//! The compensating-step executor. Define steps with a forward action and a
//! compensation; `run` executes forwards in order and, on the first forward
//! failure, runs the completed steps' compensations in reverse. Step lifecycle
//! events are emitted through a `subms-events` `EmitHandle` if one is attached.
//!
//! Scope is in-process orchestration only: durability (crash-resume),
//! distribution (remote steps + retries/timeouts), and the steps' own latency
//! are out of scope and not claimed - this is the executor, not a workflow
//! engine. Pair with `subms-ts-wal` to persist the step log.

use subms_events::{EmitHandle, Event, EventLevel};

type Action = Box<dyn Fn() -> Result<(), String> + Send + Sync>;

struct SagaStep {
    name: String,
    forward: Action,
    compensate: Action,
}

/// Overall result of a saga run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every forward step succeeded.
    Committed,
    /// A forward step failed; completed steps were compensated in reverse.
    Compensated,
}

impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Outcome::Committed => "COMMITTED",
            Outcome::Compensated => "COMPENSATED",
        }
    }
}

/// What happened when a saga ran.
#[derive(Debug, Clone)]
pub struct SagaReport {
    pub outcome: Outcome,
    /// The step whose forward failed (Compensated only).
    pub failed_step: Option<String>,
    /// The failure reason (Compensated only).
    pub reason: Option<String>,
    /// Steps whose forward completed, in run order.
    pub forward_ran: Vec<String>,
    /// Steps compensated, in reverse (rollback) order.
    pub compensated: Vec<String>,
    /// Steps whose compensation itself failed: (step, reason).
    pub compensation_failures: Vec<(String, String)>,
}

impl SagaReport {
    pub fn is_committed(&self) -> bool {
        self.outcome == Outcome::Committed
    }

    /// Deterministic JSON, byte-equivalent across the language ports.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"outcome\":");
        push_str(&mut out, self.outcome.as_str());
        if let Some(f) = &self.failed_step {
            out.push_str(",\"failed_step\":");
            push_str(&mut out, f);
        }
        if let Some(r) = &self.reason {
            out.push_str(",\"reason\":");
            push_str(&mut out, r);
        }
        out.push_str(",\"forward_ran\":");
        push_arr(&mut out, &self.forward_ran);
        if self.outcome == Outcome::Compensated {
            out.push_str(",\"compensated\":");
            push_arr(&mut out, &self.compensated);
            out.push_str(",\"compensation_failures\":[");
            for (i, (s, r)) in self.compensation_failures.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('[');
                push_str(&mut out, s);
                out.push(',');
                push_str(&mut out, r);
                out.push(']');
            }
            out.push(']');
        }
        out.push('}');
        out
    }
}

/// A saga: a named sequence of compensating steps.
pub struct Saga {
    name: String,
    steps: Vec<SagaStep>,
    emitter: Option<EmitHandle>,
}

impl Saga {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
            emitter: None,
        }
    }

    /// Emit step lifecycle events to a `subms-events` dispatcher (via its handle).
    pub fn with_emitter(mut self, emitter: EmitHandle) -> Self {
        self.emitter = Some(emitter);
        self
    }

    /// Add a step: `forward` is the action, `compensate` undoes it.
    pub fn step<F, C>(mut self, name: &str, forward: F, compensate: C) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
        C: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.steps.push(SagaStep {
            name: name.to_string(),
            forward: Box::new(forward),
            compensate: Box::new(compensate),
        });
        self
    }

    fn emit(&self, step: &str, phase: &str, reason: Option<&str>) {
        if let Some(em) = &self.emitter {
            let level = match phase {
                "forward_failed" | "compensation_failed" => EventLevel::Error,
                "compensating" | "compensated" => EventLevel::Warn,
                _ => EventLevel::Info,
            };
            let mut b = Event::builder("subms.saga")
                .level(level)
                .attr("saga", &self.name)
                .attr("step", step)
                .attr("phase", phase);
            if let Some(r) = reason {
                b = b.message(r);
            }
            em.emit(b.build());
        }
    }

    /// Run the saga. Forwards run in order; the first failure triggers reverse
    /// compensation of the completed steps.
    pub fn run(&self) -> SagaReport {
        let mut ran: Vec<usize> = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            self.emit(&step.name, "forward_started", None);
            match (step.forward)() {
                Ok(()) => {
                    ran.push(i);
                    self.emit(&step.name, "forward_completed", None);
                }
                Err(reason) => {
                    self.emit(&step.name, "forward_failed", Some(&reason));
                    let mut compensated = Vec::new();
                    let mut failures = Vec::new();
                    for &j in ran.iter().rev() {
                        let s = &self.steps[j];
                        self.emit(&s.name, "compensating", None);
                        match (s.compensate)() {
                            Ok(()) => {
                                compensated.push(s.name.clone());
                                self.emit(&s.name, "compensated", None);
                            }
                            Err(e) => {
                                failures.push((s.name.clone(), e.clone()));
                                self.emit(&s.name, "compensation_failed", Some(&e));
                            }
                        }
                    }
                    return SagaReport {
                        outcome: Outcome::Compensated,
                        failed_step: Some(step.name.clone()),
                        reason: Some(reason),
                        forward_ran: ran.iter().map(|&k| self.steps[k].name.clone()).collect(),
                        compensated,
                        compensation_failures: failures,
                    };
                }
            }
        }
        self.emit(&self.name, "committed", None);
        SagaReport {
            outcome: Outcome::Committed,
            failed_step: None,
            reason: None,
            forward_ran: ran.iter().map(|&k| self.steps[k].name.clone()).collect(),
            compensated: Vec::new(),
            compensation_failures: Vec::new(),
        }
    }
}

fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_arr(out: &mut String, items: &[String]) {
    out.push('[');
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_str(out, s);
    }
    out.push(']');
}
