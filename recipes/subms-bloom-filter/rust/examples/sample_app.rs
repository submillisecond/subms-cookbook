//! Sample app: a tour of `subms-bloom-filter`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features counting`) to see the feature
//! sections light up.
//!
//! * base        - URL-seen dedup for a web crawler's frontier
//! * merge       - per-shard filters unioned at fan-in, with occupancy readout
//! * counting    - an active-session set that supports removal (logout)
//! * scalable    - a filter that grows in layers as it fills, keeping FPR bounded
//! * partitioned - the independent-slice variant

use subms_bloom_filter::BloomFilter;

fn main() {
    base_crawler_dedup();
    shard_merge_and_occupancy();

    #[cfg(feature = "counting")]
    counting_session_set();

    #[cfg(feature = "scalable")]
    scalable_growth();

    #[cfg(feature = "partitioned")]
    partitioned_variant();
}

/// Base API: a crawler skips URLs it has already fetched. The bloom filter
/// answers "seen this?" in a fixed footprint; the only risk is a rare false
/// positive (a genuinely-new URL wrongly skipped), never a false negative.
fn base_crawler_dedup() {
    println!("== base: crawler URL dedup ==");
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
            println!("  skip  {url}");
            skipped += 1;
        } else {
            seen.add(url);
            println!("  fetch {url}");
            fetched += 1;
        }
    }
    println!("  -> fetched {fetched}, skipped {skipped}");
    for url in [
        "https://a.example/",
        "https://b.example/",
        "https://c.example/",
    ] {
        assert!(seen.might_contain(url), "no false negatives");
    }
}

/// Each shard builds its own filter over the symbols it saw, then the gateway
/// ORs them into one membership set. `union` only accepts identical geometry,
/// so every shard must be constructed with the same expected count.
/// `estimated_fpp` reports occupancy against the design point, which is how you
/// find out a filter has outgrown its sizing before the false positives do.
fn shard_merge_and_occupancy() {
    println!(
        "
== merge: per-shard filters unioned at the gateway =="
    );
    let capacity = 10_000;
    let mut gateway = BloomFilter::new(capacity);
    for shard in 0..4 {
        let mut local = BloomFilter::new(capacity);
        for i in 0..500 {
            local.add(&format!("shard{shard}-sym{i}"));
        }
        gateway.union(&local).expect("shards share one geometry");
    }
    println!("  merged 4 shards x 500 symbols");
    println!(
        "  approx distinct keys: {}",
        gateway.approximate_element_count()
    );
    println!(
        "  occupancy fpp:        {:.4}%",
        gateway.estimated_fpp() * 100.0
    );

    let mismatched = BloomFilter::new(capacity * 2);
    match gateway.union(&mismatched) {
        Err(e) => println!("  refused mismatched shard: {e}"),
        Ok(()) => unreachable!("geometry check must reject this"),
    }

    gateway.clear();
    println!(
        "  after clear -> approx distinct keys: {}",
        gateway.approximate_element_count()
    );
}

/// `counting` feature: a plain bloom filter can never remove a key. A counting
/// bloom filter keeps a small counter per cell, so a key can be removed - here,
/// an active-session set where a logout deletes the session.
#[cfg(feature = "counting")]
fn counting_session_set() {
    use subms_bloom_filter::CountingBloomFilter;
    println!("\n== counting: active sessions with logout ==");
    let mut sessions = CountingBloomFilter::new(1_000);
    for s in ["sess-alice", "sess-bob", "sess-carol"] {
        sessions.add(s);
    }
    println!("  bob active?   {}", sessions.might_contain("sess-bob"));
    sessions.remove("sess-bob"); // logout
    println!("  bob after logout? {}", sessions.might_contain("sess-bob"));
    assert!(
        sessions.might_contain("sess-alice"),
        "other sessions untouched"
    );
}

/// `scalable` feature: sized for a small capacity, it adds a fresh, larger layer
/// each time a layer fills - so the false-positive rate stays bounded no matter
/// how many keys arrive, without knowing the count up front.
#[cfg(feature = "scalable")]
fn scalable_growth() {
    use subms_bloom_filter::ScalableBloomFilter;
    println!("\n== scalable: grows past its initial capacity ==");
    let mut f = ScalableBloomFilter::new(64);
    for i in 0..1_000 {
        f.add(&format!("key-{i}"));
    }
    println!(
        "  added 1000 into a cap-64 filter -> {} layers",
        f.layer_count()
    );
    assert!(f.layer_count() > 1, "it grew");
    for i in 0..1_000 {
        assert!(
            f.might_contain(&format!("key-{i}")),
            "no false negatives after growth"
        );
    }
}

/// `partitioned` feature: instead of `k` hashes into one bit array, each hash
/// owns its own equal slice. The per-slice fill is uniform, which makes the
/// false-positive rate easier to reason about analytically.
#[cfg(feature = "partitioned")]
fn partitioned_variant() {
    use subms_bloom_filter::PartitionedBloomFilter;
    println!("\n== partitioned: one slice per hash ==");
    let mut f = PartitionedBloomFilter::new(1_000);
    for tag in ["red", "green", "blue"] {
        f.add(tag);
    }
    println!("  {} bits across {} slices", f.bit_count(), f.k());
    println!("  green present? {}", f.might_contain("green"));
    assert!(f.might_contain("red") && !f.might_contain("magenta-unseen"));
}
