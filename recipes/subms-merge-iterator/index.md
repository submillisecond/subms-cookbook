---
title: Merge iterator
summary: K-way merge of sorted streams via a min-heap of stream heads. next() is one heap-pop and one heap-push.
type: recipe
category: storage
repoPath: recipes/subms-merge-iterator
order: 16
level: L200
loc: 150
languages: [rust, java]
prereqs:
  - "Min-heaps / priority queues"
  - "Iterator protocols"
tags:
  - storage
  - merge-sort
  - low-latency
perf:
  - { label: "next p99",  value: "< 200 ns", note: "16-way merge of 10k-each streams" }
references:
  - { title: "itertools::kmerge (Rust)", url: "https://docs.rs/itertools/latest/itertools/fn.kmerge.html" }
---

Streams must be sorted ascending. The iterator maintains a min-heap keyed on `(current value, stream index)`. `next()` pops the minimum, advances the stream it came from, and pushes that stream's next value (if any). Output is the global sorted union.

The natural fit is the LSM-tree read path - one stream per SSTable, plus the memtable, merged on the fly. The recipe stays generic so the same iterator works for log-tailers, sort-merge joins, or anything else that wants k-way ordered merge.

## Quality bar

**Reference impl:** `itertools::kmerge` (Rust) for behavioural cross-check.

**Sub-ms claim under:** next p99 < 200 ns merging 16 streams of 10k entries each (heap of 16 entries; sift cost is ~log2(16) = 4 comparisons).

**Not claimed:** > 64-way merges where heap cost starts to dominate (use external sort then); adversarial key distributions that force constant heap sift to the root.
