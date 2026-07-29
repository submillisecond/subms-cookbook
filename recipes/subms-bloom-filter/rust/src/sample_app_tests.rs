//! Pins the behaviour the `sample_app` example demonstrates: the crawler-dedup
//! scenario dedups correctly and the filter never loses a URL it stored.
//! Colocated with the module and included via `#[path]` (see `lib.rs`).

use super::*;

#[test]
fn crawler_dedup_scenario() {
    let mut seen = BloomFilter::new(10_000);
    let frontier = [
        "https://a.example/",
        "https://b.example/",
        "https://a.example/", // dup
        "https://c.example/",
        "https://b.example/", // dup
    ];

    let (mut fetched, mut skipped) = (0usize, 0usize);
    for url in frontier {
        if seen.might_contain(url) {
            skipped += 1;
        } else {
            seen.add(url);
            fetched += 1;
        }
    }

    assert_eq!(fetched, 3, "three distinct URLs fetched");
    assert_eq!(skipped, 2, "two duplicates skipped");

    // The guarantee the sample leans on: no false negatives.
    for url in [
        "https://a.example/",
        "https://b.example/",
        "https://c.example/",
    ] {
        assert!(
            seen.might_contain(url),
            "a stored URL must always report present"
        );
    }
}

#[cfg(feature = "counting")]
#[test]
fn counting_supports_removal() {
    use super::CountingBloomFilter;
    let mut s = CountingBloomFilter::new(1_000);
    s.add("sess-bob");
    assert!(s.might_contain("sess-bob"));
    s.remove("sess-bob");
    assert!(!s.might_contain("sess-bob"), "removal clears membership");
}

#[cfg(feature = "scalable")]
#[test]
fn scalable_grows_and_keeps_members() {
    use super::ScalableBloomFilter;
    let mut f = ScalableBloomFilter::new(64);
    for i in 0..1_000 {
        f.add(&format!("key-{i}"));
    }
    assert!(f.layer_count() > 1, "grew past the initial layer");
    for i in 0..1_000 {
        assert!(
            f.might_contain(&format!("key-{i}")),
            "no false negatives after growth"
        );
    }
}

#[cfg(feature = "partitioned")]
#[test]
fn partitioned_membership() {
    use super::PartitionedBloomFilter;
    let mut f = PartitionedBloomFilter::new(1_000);
    f.add("red");
    assert!(f.might_contain("red"));
}
