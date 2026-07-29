//! Fuzz the hand-rolled `SagaReport::to_json` against a real JSON parser. The
//! failure reason is the riskiest field (arbitrary user text), so it is drawn
//! from a charset full of quotes, backslashes, control chars, and unicode.

use super::*;

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

const CHARS: &[char] = &[
    'a', 'Z', '0', ' ', '"', '\\', '\n', '\r', '\t', '\u{08}', '\u{01}', '/', '{', '}', ':', 'é',
    '漢', '🦀',
];

fn rand_str(rng: &mut Rng) -> String {
    let len = rng.below(14) as usize;
    (0..len)
        .map(|_| CHARS[rng.below(CHARS.len() as u64) as usize])
        .collect()
}

#[test]
fn fuzz_saga_report_json_is_valid_and_roundtrips() {
    let mut rng = Rng::new(0x5A6A);
    for _ in 0..2000 {
        let n = (1 + rng.below(6)) as usize;
        let fail = rng.below(2) == 0;
        let fail_at = rng.below(n as u64) as usize;
        let reason = rand_str(&mut rng);
        let reason_for_step = reason.clone();

        let mut s = Saga::new("p");
        for i in 0..n {
            let should_fail = fail && i == fail_at;
            let r = reason_for_step.clone();
            s = s.step(
                &format!("s{i}"),
                move || {
                    if should_fail { Err(r.clone()) } else { Ok(()) }
                },
                || Ok(()),
            );
        }
        let report = s.run();
        let json = report.to_json();

        let v: serde_json::Value =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid JSON {json:?}: {e}"));
        assert_eq!(
            v["outcome"],
            serde_json::Value::String(report.outcome.as_str().to_string())
        );
        if report.outcome == Outcome::Compensated {
            assert_eq!(v["reason"], serde_json::Value::String(reason.clone()));
            assert_eq!(
                v["failed_step"],
                serde_json::Value::String(report.failed_step.clone().unwrap())
            );
            assert!(v["compensated"].is_array());
        }
        assert!(v["forward_ran"].is_array());
    }
}
