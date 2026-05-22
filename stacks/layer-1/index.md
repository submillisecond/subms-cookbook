---
title: Layer 1 settlement
slug: layer-1
type: stack
order: 3
summary: The off-chain infrastructure around L1 block production. Block builder, validator client, mempool, MEV-bundle assembly, fork-choice watcher. Each topic carries the off-chain system that consumes the chain's primitives and produces or watches the next state transition.
topics:
  - block-builder
  - mempool
  - mev-bundle-assembly
  - validator-client
  - fork-choice-watcher
---

L1 is the consensus layer most application code treats as a black
box: "send a transaction, wait for finality, move on." Operating
at L1 means looking inside the box. A block builder assembles
transactions into a candidate block under simultaneous constraints
(gas limit, MEV optimisation, validator preference). A validator
client signs attestations and proposals against the network's
fork-choice rule. A mempool collects pending transactions from N
peer connections and de-duplicates them. An MEV bundle assembler
searches the mempool for searcher-submitted bundles and ranks them
against the builder's local candidate.

None of these are part of "the chain" - they're the off-chain
infrastructure that produces the chain's content. Each is a
performance-critical system in its own right, with sub-millisecond
expectations during the slot in which a block must be produced.

This stack covers the off-chain infrastructure. The cryptography
(BLS signatures, KZG commitments, etc.) and the consensus rules
themselves are out of scope - those are the protocol-level concerns
documented elsewhere.

## When to reach for what

| Symptom | Reach for |
|---|---|
| Builder latency past the slot's bid-window | [Block builder](./block-builder) - the bundle-selection + state-execution loop is the bottleneck; the lock-free MEV ranking + arena scratch is the fix |

(More L1 topics to be added: mempool, MEV-bundle assembly, validator
client, fork-choice watcher.)

## Out of scope

- **Consensus protocol itself.** Slashing rules, finality gadgets,
  fork-choice mathematics. The cookbook does not relitigate
  Ethereum's Casper FFG or competing designs.
- **Cryptographic primitives.** BLS, KZG, BN254 pairings. Use
  audited reference libraries; the cookbook's contribution is the
  systems work that consumes them.
- **Wallet / signer code.** Key management, EIP-712 signing flows.
  Different audience question.
