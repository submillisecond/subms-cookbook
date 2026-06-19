//! Property test: the saga compensation invariant must hold for any number of
//! steps and any failure point. Committed iff no forward fails; otherwise the
//! completed prefix is compensated in exact reverse order.

use subms_events_saga::{Outcome, Saga};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn ok() -> Result<(), String> {
    Ok(())
}

#[test]
fn prop_compensation_invariant() {
    let mut rng = Rng::new(11);
    for _ in 0..1000 {
        let n = (1 + rng.below(8)) as usize;
        let fail: Option<usize> = if rng.below(2) == 0 {
            Some(rng.below(n as u64) as usize)
        } else {
            None
        };

        let mut s = Saga::new("p");
        for i in 0..n {
            let should_fail = fail == Some(i);
            s = s.step(
                &format!("s{i}"),
                move || {
                    if should_fail {
                        Err("x".to_string())
                    } else {
                        Ok(())
                    }
                },
                ok,
            );
        }
        let r = s.run();

        match fail {
            None => {
                assert_eq!(r.outcome, Outcome::Committed);
                let all: Vec<String> = (0..n).map(|i| format!("s{i}")).collect();
                assert_eq!(r.forward_ran, all);
                assert!(r.compensated.is_empty());
            }
            Some(f) => {
                assert_eq!(r.outcome, Outcome::Compensated);
                assert_eq!(r.failed_step.as_deref(), Some(format!("s{f}").as_str()));
                let ran: Vec<String> = (0..f).map(|i| format!("s{i}")).collect();
                assert_eq!(r.forward_ran, ran);
                let mut comp = ran.clone();
                comp.reverse();
                assert_eq!(r.compensated, comp); // exact reverse-order rollback
            }
        }
    }
}
