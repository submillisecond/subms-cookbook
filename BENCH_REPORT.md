# Cookbook bench report

Generated 2026-05-23T20:33:35.253Z
Threshold: p99 < 1000us

| Recipe | Rust | Rust p99 | Java | Java p99 | Worst stage |
|---|---|---|---|---|---|
| subms-adaptive-radix-tree | ✓ | 9.5us | ✓ | 2.1us | `insert` (9.5us) |
| subms-arena-allocator | ✓ | 200ns | ✓ | 200ns | `allocate` (0.2us) |
| subms-block-cache | ✓ | 300ns | ✓ | 1.1us | `put` (1.1us) |
| subms-bloom-filter | ✓ | 200ns | ✓ | 300ns | `add` (0.3us) |
| subms-count-min-sketch | ✓ | 200ns | ✓ | 300ns | `estimate` (0.3us) |
| subms-cuckoo-filter | ✓ | 200ns | ✓ | 300ns | `insert` (0.3us) |
| subms-hdr-histogram | ✓ | 200ns | ✓ | 49us | `percentile` (48.7us) |
| subms-hyperloglog | ✓ | 200ns | ✓ | 228us | `estimate` (227.5us) |
| subms-lsm-tree | ✓ | 28us | ✓ | 34us | `get_hit` (34.3us) |
| subms-merge-iterator | ✓ | 200ns | ✓ | 400ns | `next` (0.4us) |
| subms-mpsc-queue | ✓ | 2.1us | ✓ | 1.3us | `offer` (2.1us) |
| subms-perf-gate | ∅ | - | ∅ | - | - |
| subms-rate-limiter | ✓ | 20us | ✓ | 8.1us | `try_acquire` (20.4us) |
| subms-segment-reader | ✓ | 200ns | ✓ | 200ns | `next_record` (0.2us) |
| subms-spsc-ring-buffer | ✓ | 200ns | ✓ | 400ns | `enqueue` (0.4us) |
| subms-timer-wheel | ✓ | 4.7us | ✓ | 9.2us | `tick` (9.2us) |
| subms-treap | ✓ | 3.0us | ✓ | 11us | `insert` (10.9us) |

## Per-stage breakdown

### subms-adaptive-radix-tree

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | adaptive-radix-tree | `insert` | 600ns | 9.5us | 36us | 36us |  |
| rust | adaptive-radix-tree | `lookup` | 300ns | 1.4us | 3.9us | 3.9us |  |
| java | adaptive-radix-tree | `insert` | 700ns | 1.8us | 2.9us | 2.9us |  |
| java | adaptive-radix-tree | `lookup` | 700ns | 2.1us | 7.5us | 7.5us |  |

### subms-arena-allocator

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | arena-allocator | `allocate` | 100ns | 200ns | 1.1us | 1.1us |  |
| rust | arena-allocator | `reset` | 100ns | 200ns | 200ns | 200ns |  |
| java | arena-allocator | `allocate` | 100ns | 200ns | 200ns | 200ns |  |
| java | arena-allocator | `reset` | 100ns | 200ns | 200ns | 200ns |  |

### subms-block-cache

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | block-cache | `get` | 100ns | 200ns | 200ns | 200ns |  |
| rust | block-cache | `put` | 100ns | 300ns | 300ns | 300ns |  |
| java | block-cache | `get` | 100ns | 300ns | 500ns | 500ns |  |
| java | block-cache | `put` | 400ns | 1.1us | 8.9us | 8.9us |  |

### subms-bloom-filter

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | bloom-filter | `add` | 100ns | 200ns | 800ns | 800ns |  |
| rust | bloom-filter | `might_contain_hit` | 100ns | 200ns | 300ns | 300ns |  |
| rust | bloom-filter | `might_contain_miss` | 100ns | 200ns | 200ns | 200ns |  |
| java | bloom-filter | `add` | 100ns | 300ns | 2.8us | 2.8us |  |
| java | bloom-filter | `might_contain_hit` | 100ns | 300ns | 1.4us | 1.4us |  |
| java | bloom-filter | `might_contain_miss` | 100ns | 300ns | 2.7us | 2.7us |  |

### subms-count-min-sketch

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | count-min-sketch | `add` | 100ns | 200ns | 200ns | 200ns |  |
| rust | count-min-sketch | `estimate` | 100ns | 200ns | 1.3us | 1.3us |  |
| java | count-min-sketch | `add` | 100ns | 200ns | 200ns | 200ns |  |
| java | count-min-sketch | `estimate` | 100ns | 300ns | 700ns | 700ns |  |

### subms-cuckoo-filter

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | cuckoo-filter | `insert` | 100ns | 200ns | 200ns | 200ns |  |
| rust | cuckoo-filter | `contains` | 100ns | 200ns | 1.3us | 1.3us |  |
| rust | cuckoo-filter | `delete` | 100ns | 200ns | 1.2us | 1.2us |  |
| java | cuckoo-filter | `insert` | 100ns | 300ns | 800ns | 800ns |  |
| java | cuckoo-filter | `contains` | 100ns | 300ns | 14us | 14us |  |
| java | cuckoo-filter | `delete` | 100ns | 300ns | 7.3us | 7.3us |  |

### subms-hdr-histogram

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | hdr-histogram | `record` | 100ns | 200ns | 200ns | 200ns |  |
| rust | hdr-histogram | `percentile` | 100ns | 200ns | 200ns | 200ns |  |
| java | hdr-histogram | `record` | 100ns | 200ns | 300ns | 300ns |  |
| java | hdr-histogram | `percentile` | 46us | 49us | 49us | 49us |  |

### subms-hyperloglog

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | hyperloglog | `add` | 100ns | 200ns | 200ns | 200ns |  |
| rust | hyperloglog | `estimate` | 100ns | 100ns | 100ns | 100ns |  |
| java | hyperloglog | `add` | 100ns | 200ns | 600ns | 600ns |  |
| java | hyperloglog | `estimate` | 74us | 228us | 228us | 228us |  |

### subms-lsm-tree

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | lsm-tree | `put` | 200ns | 700ns | 3.9us | 3.9us |  |
| rust | lsm-tree | `get_hit` | 6.0us | 23us | 47us | 47us |  |
| rust | lsm-tree | `get_miss` | 2.6us | 28us | 38us | 38us |  |
| java | lsm-tree | `put` | 400ns | 1.1us | 26us | 26us |  |
| java | lsm-tree | `get_hit` | 8.3us | 34us | 111us | 111us |  |
| java | lsm-tree | `get_miss` | 2.4us | 28us | 55us | 55us |  |
| java | lsm-tree | `put` | 400ns | 900ns | 1.4us | 1.4us | yes |
| java | lsm-tree | `get_hit` | 13us | 978us | 1.15ms | 1.15ms | yes |
| java | lsm-tree | `get_miss` | 800us | 1.52ms | 2.53ms | 2.53ms | yes |

### subms-merge-iterator

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | merge-iterator | `next` | 100ns | 200ns | 200ns | 200ns |  |
| java | merge-iterator | `next` | 200ns | 400ns | 5.3us | 5.3us |  |

### subms-mpsc-queue

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | mpsc-queue | `offer` | 600ns | 2.1us | 7.0us | 7.0us |  |
| rust | mpsc-queue | `poll` | 100ns | 600ns | 1.1us | 1.1us |  |
| java | mpsc-queue | `offer` | 300ns | 1.3us | 20us | 20us |  |
| java | mpsc-queue | `poll` | 100ns | 200ns | 200ns | 200ns |  |

### subms-rate-limiter

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | rate-limiter | `try_acquire` | 600ns | 20us | 81us | 81us |  |
| java | rate-limiter | `try_acquire` | 300ns | 8.1us | 14us | 14us |  |

### subms-segment-reader

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | segment-reader | `next_record` | 100ns | 200ns | 200ns | 200ns |  |
| java | segment-reader | `next_record` | 100ns | 200ns | 400ns | 400ns |  |

### subms-spsc-ring-buffer

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | spsc-ring-buffer | `enqueue` | 100ns | 200ns | 800ns | 800ns |  |
| rust | spsc-ring-buffer | `dequeue` | 100ns | 200ns | 1.2us | 1.2us |  |
| java | spsc-ring-buffer | `enqueue` | 100ns | 400ns | 500ns | 500ns |  |
| java | spsc-ring-buffer | `dequeue` | 100ns | 300ns | 400ns | 400ns |  |

### subms-timer-wheel

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | timer-wheel | `schedule` | 200ns | 1.3us | 13us | 13us |  |
| rust | timer-wheel | `cancel` | 300ns | 600ns | 7.2us | 7.2us |  |
| rust | timer-wheel | `tick` | 1.0us | 4.7us | 11us | 11us |  |
| java | timer-wheel | `schedule` | 200ns | 400ns | 600ns | 600ns |  |
| java | timer-wheel | `cancel` | 500ns | 2.4us | 35us | 35us |  |
| java | timer-wheel | `tick` | 1.4us | 9.2us | 20us | 20us |  |

### subms-treap

| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |
|---|---|---|---|---|---|---|---|
| rust | treap | `insert` | 400ns | 3.0us | 8.3us | 8.3us |  |
| rust | treap | `lookup` | 300ns | 700ns | 1.2us | 1.2us |  |
| java | treap | `insert` | 900ns | 11us | 140us | 140us |  |
| java | treap | `lookup` | 700ns | 2.1us | 5.3us | 5.3us |  |
