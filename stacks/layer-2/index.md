---
title: Layer 2 rollups
slug: layer-2
type: stack
order: 4
summary: The off-chain infrastructure that runs L2 rollups. Sequencer, fraud-proof watcher, bridge messenger, forced-inclusion mempool. Each topic carries a system that produces, verifies, or enforces an L1-anchored L2 chain.
topics:
  - rollup-sequencer
  - fraud-proof-watcher
  - bridge-messenger
  - forced-inclusion
  - state-witness-server
---

An L2 rollup is an off-chain execution environment whose state
transitions are anchored to an L1 contract. The L2 itself runs on
infrastructure that's structurally different from L1: a sequencer
produces blocks (typically a single operator's process, not a
network of validators), batches them into a calldata or blob
submission, posts to L1, and either (a) waits for the optimistic
challenge window to close, or (b) submits a validity proof
immediately.

This stack covers the operational systems that make rollups work.
Sequencer (the block-producing core), fraud-proof watcher (the
trust-minimised role that watches for misbehaving sequencers and
submits challenges), bridge messenger (the cross-domain message
passing layer), forced-inclusion mempool (the censorship-resistance
escape hatch via L1).

The cryptographic primitives (ZK proofs in zk-rollups, fault-proof
games in optimistic rollups, blob commitment in EIP-4844) are out
of scope. Use audited reference libraries; the cookbook's
contribution is the systems work around them.

## When to reach for what

| Symptom | Reach for |
|---|---|
| Sequencer falls behind during throughput burst | [Rollup sequencer](./rollup-sequencer) - the bottleneck is usually the per-block execution + state-commitment loop, not the tx ingest |

(More L2 topics to be added: fraud-proof watcher, bridge messenger,
forced-inclusion mempool, state-witness server.)

## Out of scope

- **Cryptography of the proving system.** ZK circuits, fault-proof
  games. Audited reference libraries assumed.
- **L1 contracts of the rollup.** The on-chain message-passing
  contracts, the proof-verification contracts, the bridge
  contracts. These live in the rollup's deployment repo; the
  cookbook contribution is the off-chain systems that talk to
  them.
- **DA layer choice.** Whether the rollup posts to L1 calldata,
  EIP-4844 blobs, Celestia, EigenDA, etc. is an architectural
  choice; the systems here work against any.
