---
title: Smart contracts
slug: smart-contracts
type: stack
order: 2
summary: The on-chain code + the off-chain infrastructure that makes it operable. Vault accounting, event indexer, mempool watcher, simulator, state-diff watcher, governance + timelock. Each topic carries the on-chain pattern and the off-chain support system that watches, decodes, and reacts to it.
topics:
  - vault-accounting
  - event-indexer
  - mempool-watcher
  - simulator
  - state-diff-watcher
  - governance-timelock
---

A smart contract is half the system. The other half is the off-chain
infrastructure that watches it: indexers that decode events into
queryable state, mempool watchers that classify pending transactions,
simulators that fork the current state for what-if analysis,
state-diff watchers that catch unexpected mutations between blocks.
Without that infrastructure, the contract is opaque.

This stack pairs each on-chain pattern with the off-chain system it
needs. The contract code lives where you'd expect (your Solidity
repo); the cookbook's contribution is the off-chain plumbing - the
queues, indexes, simulators, and observability primitives that turn
on-chain code into something you can operate.

## When to reach for what

| Symptom | Reach for |
|---|---|
| LP share calculation drifts from on-chain reserves | [Vault accounting](./vault-accounting) - the off-chain reconciler catches it within one block |
| Missed a contract event during deployment | [Event indexer](./event-indexer) - replay-safe consumer + bloom dedup |
| Pending tx classification is the bottleneck | [Mempool watcher](./mempool-watcher) - SPSC-per-RPC + bloom-by-signature |
| Need to know "what would happen if I called this" | [Simulator](./simulator) - fork-state execution with budget tracking |
| Unexpected state change between blocks | [State-diff watcher](./state-diff-watcher) - per-slot change feed |
| Governance proposal accidentally clobbers state | [Governance + timelock](./governance-timelock) - off-chain dry-run before timelock unlock |

## Out of scope

- **Compiler-level optimisation.** Storage packing, function selector
  ordering, calldata vs memory tradeoffs - those are Solidity-side
  craft; cookbook doesn't compete with the EVM reference docs there.
- **L1 / L2 settlement.** Those are their own stacks - the contract
  layer assumes block finality is delivered by the appropriate
  underlying stack.
- **Audit + formal verification.** Process question, not an
  infrastructure question.
