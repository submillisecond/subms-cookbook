//! Fuzz the hand-rolled `Event::to_json` against a real JSON parser. For
//! thousands of random events (including quotes, backslashes, control chars,
//! and unicode), the output must parse as valid JSON and round-trip every field.

use std::collections::BTreeMap;

use subms_events::{Event, EventLevel};

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

// A charset chosen to stress the escaper: quote, backslash, control chars,
// slashes, braces, and multi-byte unicode.
const CHARS: &[char] = &[
    'a', 'Z', '0', ' ', '"', '\\', '\n', '\r', '\t', '\u{08}', '\u{0c}', '\u{01}', '/', '{', '}',
    ':', ',', 'é', '漢', '🦀',
];

fn rand_str(rng: &mut Rng) -> String {
    let len = rng.below(12) as usize;
    (0..len)
        .map(|_| CHARS[rng.below(CHARS.len() as u64) as usize])
        .collect()
}

const LEVELS: [EventLevel; 5] = [
    EventLevel::Trace,
    EventLevel::Debug,
    EventLevel::Info,
    EventLevel::Warn,
    EventLevel::Error,
];

#[test]
fn fuzz_event_json_is_valid_and_roundtrips() {
    let mut rng = Rng::new(0xC0FFEE);
    for _ in 0..3000 {
        let topic = rand_str(&mut rng);
        let at = rand_str(&mut rng);
        let level = LEVELS[rng.below(5) as usize];
        let mut b = Event::builder(&topic).level(level).at(&at);
        let has_msg = rng.below(2) == 0;
        let msg = rand_str(&mut rng);
        if has_msg {
            b = b.message(&msg);
        }
        let mut attrs: BTreeMap<String, String> = BTreeMap::new();
        for _ in 0..rng.below(4) {
            let k = rand_str(&mut rng);
            let v = rand_str(&mut rng);
            attrs.insert(k.clone(), v.clone());
            b = b.attr(&k, &v);
        }
        let e = b.build();
        let json = e.to_json();

        let v: serde_json::Value = serde_json::from_str(&json)
            .unwrap_or_else(|err| panic!("invalid JSON {json:?}: {err}"));
        assert_eq!(v["topic"], serde_json::Value::String(topic));
        assert_eq!(
            v["level"],
            serde_json::Value::String(level.as_str().to_string())
        );
        assert_eq!(v["at"], serde_json::Value::String(at));
        if has_msg {
            assert_eq!(v["message"], serde_json::Value::String(msg));
        } else {
            assert!(v.get("message").is_none());
        }
        for (k, val) in &attrs {
            assert_eq!(v["attributes"][k], serde_json::Value::String(val.clone()));
        }
    }
}
