---
title: Log and segment primitives
summary: Append-only segments, framed reads, and the block cache that sits in front. The bytes-on-disk side of every log-structured system.
type: topic
order: 5
recipes:
  - recipes/subms-segment-reader
  - recipes/subms-merge-iterator
  - recipes/subms-block-cache
tags:
  - storage
  - log
---

A log-structured system is a sequence of length-prefix framed records written into rotating segment files, fronted by an in-memory block cache, and merged on read with the next segment in the sort order. Every component is small. None of them is hard. All of them are subtly wrong in most public implementations.

The three recipes in this topic give you the read side of that pipeline:

- **Segment reader** opens an mmap-backed segment, walks length-prefix + CRC framed records, surfaces a typed `next()` that costs ~1 microsecond per record on a warm page cache. The cold-cache path is hardware-bound and documented as such.
- **Merge iterator** combines N sorted segment streams via a tournament tree (or a min-heap; both ship). next() is sub-microsecond up to ~16-way merges; beyond that, external sort is the right answer and the recipe says so.
- **Block cache** is the LRU + clock-sweep cache fronting the segments. Constant-time eviction; hit-path is a hash probe and a counter bump. Pairs with the segment reader to keep hot pages resident.

If you also need durability, see the LSM tree recipe in [[ordered-indexes]] - it composes these three with an in-memory memtable and a flush policy.
