use super::*;

#[test]
fn display_names_the_two_precisions() {
    let e = HllError::PrecisionMismatch {
        left: 14,
        right: 12,
    };
    assert_eq!(e.to_string(), "precision mismatch: 14 vs 12");
}

#[test]
fn display_covers_every_variant() {
    let all = [
        HllError::PrecisionMismatch { left: 4, right: 5 },
        HllError::InvalidPrecision(30),
        HllError::BadMagic,
        HllError::UnsupportedVersion(9),
        HllError::UnsupportedEncoding(7),
        HllError::Truncated {
            expected: 24,
            actual: 8,
        },
    ];
    for e in all {
        let s = e.to_string();
        assert!(!s.is_empty(), "{e:?} has an empty message");
        assert!(s.is_ascii(), "{e:?} message is not ASCII");
    }
}

#[test]
fn implements_std_error() {
    fn as_error(e: HllError) -> Box<dyn std::error::Error> {
        Box::new(e)
    }
    let boxed = as_error(HllError::BadMagic);
    assert!(boxed.to_string().contains("magic"));
}
