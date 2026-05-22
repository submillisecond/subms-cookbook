---
title: Bloom filter
summary: A space-efficient probabilistic set - "definitely not present" in seven hash probes. The cheap negative answer that makes LSM tree reads fast.
type: recipe
category: data-structures
repoPath: recipes/subms-bloom-filter
order: 5
level: L100
loc: 200
languages: [rust, java]
stacks: [defi]
prereqs:
  - "Hashing and hash distribution"
  - "Bit manipulation"
glossary:
  - false-sharing
tags:
  - data-structures
  - probabilistic-data-structures
  - low-latency
perf:
  - { label: "bits/key",       value: "10",       note: "~1% false-positive rate at the optimum" }
  - { label: "hashes/probe",   value: "7",        note: "double-hashed from one FNV-1a output" }
  - { label: "miss p99 (Rust)", value: "~16 µs",  note: "measured inside a 50k-entry LSM with 46 SSTables; the filter is the difference between this and ~620 µs without it" }
  - { label: "miss p99 (Java)", value: "~35 µs",  note: "same workload; ~1.2 ms without the filter" }
references:
  - { title: "LSM tree (this cookbook)", url: "/cookbook/recipes/subms-lsm-tree", note: "the consumer - one bloom per SSTable trailer" }
---

A **bloom filter** answers one question fast: *is this key definitely not in the set?* It can say "definitely not" or "maybe yes". False positives are possible at a tunable rate; false negatives are not. That asymmetric answer is exactly the shape you want when a real lookup is expensive and a fast negative can short-circuit it - SSTable scans, cache miss filters, log dedup checks, "have we seen this user yet" gates.

## The shape

```mermaid
flowchart LR
  K[key] --> H[FNV-1a 64-bit hash]
  H --> S[split into h1, h2]
  S --> P[probe i: h1 + i*h2 mod m]
  P -->|all bits set| Y[might contain]
  P -->|any bit zero| N[definitely not]
```

A fixed-size bit array of `m` bits and `k` hash functions. To add a key: set `k` bits, one per hash. To probe a key: check the same `k` bits - if any is zero, the key was never added; if all are set, the key was probably added (but might be a collision with previous adds - that's the false positive).

## Sizing

The standard optimum for false-positive rate `p` and `n` entries:

| variable | optimal value          | meaning                |
|----------|------------------------|------------------------|
| `m`      | `-n · ln(p) / (ln 2)²` | bits in the array      |
| `k`      | `(m / n) · ln 2`       | number of hash probes  |

At `p = 0.01` this collapses to a memorable constant pair: **~10 bits per key, k = 7**. Both implementations hard-code these constants - the cookbook value is in seeing how few moving parts the structure actually has, not in tuning.

## The double-hashing trick

A naive bloom filter evaluates `k` separate hash functions per `add`/`probe` - wasteful when `k = 7`. Instead, compute one 64-bit hash, split it into two 32-bit halves `h1` and `h2`, and derive every probe from those two:

| probe index            | bit position                       |
|------------------------|------------------------------------|
| `i ∈ 0..k`             | `(h1 + i · h2) mod m`              |

Two hash evaluations per call instead of seven. The trick is sound (Kirsch & Mitzenmacher 2006) and what every production bloom does. We use **FNV-1a 64-bit** as the underlying mixer - a five-line non-cryptographic hash that's deterministic across languages.

## Wire format

Identical across Java and Rust so a filter written by one can be read by the other:

<div class="smm-glance">
  <header class="smm-glance-head">
    <span class="smm-glance-dot" aria-hidden></span>
    <span class="smm-glance-label">on-disk layout</span>
  </header>
  <ul class="smm-glance-grid">
    <li class="smm-glance-cell">
      <span class="smm-glance-cell-label">bit_count</span>
      <span class="smm-glance-cell-value">u32 BE</span>
      <span class="smm-glance-cell-note">total bits in the array</span>
    </li>
    <li class="smm-glance-cell">
      <span class="smm-glance-cell-label">k</span>
      <span class="smm-glance-cell-value">u32 BE</span>
      <span class="smm-glance-cell-note">probes per add / lookup</span>
    </li>
    <li class="smm-glance-cell">
      <span class="smm-glance-cell-label">word_count</span>
      <span class="smm-glance-cell-value">u32 BE</span>
      <span class="smm-glance-cell-note">number of u64 words that follow</span>
    </li>
    <li class="smm-glance-cell">
      <span class="smm-glance-cell-label">bits</span>
      <span class="smm-glance-cell-value">u64 BE × word_count</span>
      <span class="smm-glance-cell-note">the bit array itself, packed</span>
    </li>
  </ul>
</div>

A 1000-key filter is 12 B header + ~1.25 KB bits. Cheap.

## What you get for free

- **Sub-microsecond `mightContain`.** Seven memory accesses against a small bit array. Branch-predictable.
- **Deterministic across languages.** Same FNV-1a, same bit layout, same probe formula.
- **Trivial serialisation.** Sized once, immutable thereafter.

## What you have to engineer

- **Hash quality.** A bad hash clusters bits and inflates the FPR. FNV-1a is fine for non-cryptographic use; if your keys are adversarial, swap for SipHash.
- **`(h2 | 1)`.** If the upper 32 bits happen to be zero, every probe lands at index `h1` and the filter degenerates to one hash. Force the low bit to 1.
- **Don't size for `expected_entries = 0`.** The 64-bit floor in `BloomFilter::new(n)` is what stops `m = 0` from blowing the modulus.

## Common pitfalls

- **False negatives "feel like a bug".** They aren't possible. If `mightContain` returns false, the key was never added - full stop. If you observe one, the real cause is an aliased buffer, a write you forgot to commit, or two filter instances pointed at different bits.
- **Removing keys.** A standard bloom can't unset a bit without risking false negatives for other keys that share it. If you need deletes, use a *counting* bloom (bit array → 4-bit counters).
- **Resizing.** A filter is immutable in size. To grow, build a new one and re-add every key from the source of truth (you do have a source of truth, right?).

## Consumed by

[LSM tree](/cookbook/recipes/subms-lsm-tree) - one bloom filter per SSTable, parsed out of the file's trailer. Lets a get short-circuit before scanning the records. The performance figures above come from that integration: with bloom on, miss p99 is ~16 µs (Rust) / ~35 µs (Java); off, it blows past 1 ms.
