---
title: Segment reader
summary: Read length-prefix framed records from a segment file. Truncated tails surface as a typed error, not a crash.
type: recipe
category: storage
repoPath: recipes/subms-segment-reader
order: 17
level: L200
loc: 220
languages: [rust, java]
prereqs:
  - "Binary framing (length-prefix vs delimiter)"
  - "Big-endian wire formats"
  - "Crash recovery and torn-tail handling"
tags:
  - storage
  - log
  - framing
perf:
  - { label: "next() p99", value: "< 1 us", note: "warm page cache; in-memory ByteArray input" }
references:
  - { title: "Kafka log segment format", url: "https://kafka.apache.org/documentation/#log", note: "real-world reference for the framed-record pattern" }
---

Frame format on disk: `u32 length (big-endian)` then `length` bytes of payload. Reads stream record-by-record. A clean EOF returns `Ok(None)` / `null`. A header or payload cut mid-frame returns a typed `TruncatedFrame` error - the recipe assumes crash recovery is a common operational case and the API treats torn-tails as expected rather than a panic.

## Quality bar

**Reference impl:** Kafka log segments for the framing model; the same length-prefix pattern as the cookbook LSM SSTable.

**Sub-ms claim under:** next() p99 < 1 us on a warm page cache with in-memory input.

**Not claimed:** cold-cache reads (sub-ms assumes pages resident; first-touch is hardware-bound); the write side (this is the read half).
