---
title: Arena allocator
summary: Bump-pointer arena with chunked growth and reset() for per-request reuse. Allocate is ~10 ns; reset is constant-time.
type: recipe
category: memory
repoPath: recipes/subms-arena-allocator
order: 10
level: L100
loc: 200
languages: [rust, java]
stacks: [defi]
prereqs:
  - "Alignment math and pointer arithmetic"
  - "Manual memory management (Rust) / off-heap byte buffers (Java)"
tags:
  - memory
  - allocator
  - low-latency
perf:
  - { label: "allocate p99",  value: "< 100 ns", note: "single bump on warm chunk" }
  - { label: "reset p99",     value: "< 100 ns", note: "constant-time pointer rewind" }
references:
  - { title: "bumpalo (Rust)", url: "https://crates.io/crates/bumpalo", note: "canonical Rust bump arena; ships in rustc itself" }
  - { title: "JDK 22 Foreign Memory API", url: "https://docs.oracle.com/en/java/javase/22/docs/api/java.base/java/lang/foreign/Arena.html", note: "the right Java analogue for off-heap arenas in production" }
---

A bump-pointer arena hands out memory by incrementing a cursor. No free list, no per-allocation metadata. When the request would overflow the current buffer, the arena allocates a new chunk at twice the previous size. `reset()` rewinds the cursor without freeing the buffer; the next request reuses the same memory.

Two footguns:

- **Drop is not run** on items in the arena (Rust). Allocate a `String`, the heap buffer for it leaks. This recipe keeps the public surface restricted to `Copy` types via `alloc_copy<T: Copy>`. If you need destructors, you need a typed arena (a separate recipe).
- **Alignment math** is `(p + align - 1) & !(align - 1)`. The naive `p + (align - p % align)` form is wrong for `align == 1`. Both implementations show the correct form and an `assert!(align.is_power_of_two())`.

## Quality bar

**Reference impl:** `bumpalo` (Rust); JDK 22 `java.lang.foreign.Arena` for the production Java pattern.

**Sub-ms claim under:** allocate p99 < 100 ns on a warm chunk; reset p99 < 100 ns (constant-time pointer rewind); arena-vs-heap speedup 10-20x on typical short-lived workloads.

**Not claimed:** long-lived allocations inside one arena (the win disappears when reset never fires); cross-thread arena sharing without explicit shared-arena lifecycle.
