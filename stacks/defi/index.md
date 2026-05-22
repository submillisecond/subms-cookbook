---
title: DeFi backend
slug: defi
type: stack
order: 1
summary: A sub-millisecond DeFi backend assembled from cookbook recipes. Order book matching, AMM pricing, risk engine, position health, liquidation watch, price oracle, mark-price oracle, insurance fund + ADL, market data fanout, funding rate. Nine topics; every order, fill, and price tick goes through this graph.
topics:
  - order-book-matching
  - liquidity-pool
  - amm
  - risk-engine
  - position-health
  - liquidation
  - insurance-fund
  - price-oracle
  - market-data-fanout
  - funding-rate
---

The DeFi backend has a tighter latency envelope than almost any
other system on the modern web. An order has to be risk-checked,
matched, settled, and acknowledged before the next block locks in
either a better or a worse price. A liquidation has to fire before
the move that triggered it continues. A funding payment has to
settle atomically across every open position at the interval
boundary. None of this is a single algorithm; it's nine cooperating
components, each with its own contract, each with a sub-millisecond
budget that downstream consumers depend on.

This stack is those nine topics + the cookbook recipes each one
composes from. The recipes carry the runnable code + the runnable
p99 assertion; this stack is the integration shape that holds them
together.

## The data flow

Three components fan out from one. The **price oracle** is the
shared truth - everything downstream reads it. The **risk engine**
gates writes - every order goes through its pre-trade check before
it can hit the book. The **matching engine** is the central
mutator - every position state change originates here.

```
                            +--------------+
   Upstream sources --------> Price oracle | <-- mark-price source
                            +------+-------+
                                   |
              +--------------------+--------------------+
              |                    |                    |
      +-------v------+    +--------v-------+    +-------v-------+
      | Risk engine  |    | Position health |    | Funding rate  |
      +-------+------+    +--------+-------+    +-------+-------+
              |                    ^                    |
              v                    |                    |
      +-------+------+             |                    |
      | Order book   |             |                    |
      | matching     +-------------+                    |
      +------+-------+   fills                          |
             |                                          |
             v                                          v
      +------+------+    +-------------+    +-----------+----------+
      | Liquidation +--->| Insurance   |    | Market data fanout   |
      | watch       |    | fund + ADL  |    |                      |
      +------+------+    +-----+-------+    +----------+-----------+
             |                 |                       |
             +---force-close---+                       |
                               v                       v
                                              All subscribers
                                              (traders, makers,
                                              indexers, audit)
```

The **AMM pricing** topic is an alternative entry point. Spot-style
pools read the oracle for initial price + circuit-breaker veto, do
their own quote arithmetic, and emit fills that take the same path
as order-book trades from there.

## When to reach for what

Match the symptom you're hitting to the topic that owns the recovery.
None of these are subtle; if you've operated a DeFi backend at scale
you'll recognise each one.

| Symptom | Reach for |
|---|---|
| Quote latency degrades under burst load | [AMM pricing](./amm) - read-path coordination is the bottleneck; packed-u128 reserves on a lock-free read are the fix. |
| Match latency degrades on a popular symbol | [Order book matching](./order-book-matching) - per-symbol SPSC + treap price-level index keeps matches deterministic; arena scratch keeps per-match cost flat. |
| Pre-trade checks are the slow path | [Risk engine](./risk-engine) - bloom-filter short-circuit + ART traversal cuts the 95% case to one hash test. |
| Position PnL display flickers or lags during a tick | [Position health](./position-health) - the per-symbol reverse-holders ART + SPSC tick channel is the recompute fast path. |
| Bad debt accumulates during sharp moves | [Liquidation watch](./liquidation) - recheck cadence too loose; timer-wheel + hot-position sketch retargets cadence per-position. |
| Solvent positions liquidated during oracle jitter | [Liquidation watch](./liquidation) + [Price oracle](./price-oracle) - missing piece is the circuit-breaker handshake between them. |
| Cascading liquidations flood the mempool | [Liquidation watch](./liquidation) - trigger rate limiter caps on-chain submissions so the keeper backlog has a known max depth. |
| Insurance fund draining without ADL firing | [Insurance fund + ADL](./insurance-fund) - the coordinator's MPSC must serialise drain + ADL-selection on one writer; any read-then-write race here is bad debt. |
| Stale prices from a flaky upstream source | [Price oracle](./price-oracle) - per-source bloom drops duplicate replays; per-source histogram surfaces the slow feed. |
| Median jumps on a single-source spike | [Price oracle](./price-oracle) - cross-source circuit breaker freezes the median when any source diverges past threshold. |
| A slow subscriber is starving the fast ones | [Market data fanout](./market-data-fanout) - high-water-mark on the per-subscriber ring transitions them into snapshot mode without back-pressuring the publisher. |
| Funding rate jumps suspiciously at interval boundary | [Funding rate](./funding-rate) - sample-variance gate refuses to publish when the window's variance exceeds threshold, defeating boundary manipulation. |
| p99 chart looks fine but tail still spikes | All nine - [`subms-hdr-histogram`](/cookbook/recipes/subms-hdr-histogram) with coordinated-omission backfill is the only honest measurement here. |

## How the nine compose

The shared invariants:

- **The price oracle is the single source of truth.** Every other
  component that needs a price reads from it. No component reads an
  upstream source directly; the dedup + median + circuit-breaker
  must always run before any consumer sees a value.
- **The risk engine is the single pre-trade gate.** No order
  reaches the matching engine without going through risk; conversely,
  the matching engine never re-checks margin. Risk has the up-to-date
  exposure snapshot for that.
- **Settlement runs on the consumer side.** When a fill happens, the
  matching engine emits a fill event; position-health updates the
  account state; insurance-fund-+-ADL watches for liquidation
  triggers; market-data-fanout streams to subscribers. The matching
  engine itself does NOT update account state - that's the settlement
  worker's job.
- **One writer per piece of state.** The insurance fund balance has
  one writer (the coordinator). Each account's positions have one
  writer (the settlement worker keyed on that account). The mark
  prices are written by the mark-price oracle. Locks are unnecessary
  by construction; the architecture, not the primitive, enforces the
  invariant.
- **HDR histogram everywhere.** Every component exports a per-event
  latency histogram with CO backfill. Naive percentiles will smooth
  the exact moments you most need to see.

## Out of scope

This stack does not cover:

- **On-chain settlement.** Block builders, MEV bundles, bridge
  finalisers, sequencer integration. The recipes here are CPU +
  memory primitives, not consensus.
- **Cross-venue order matching.** Each matching engine prices its
  own book; cross-venue routing is a downstream system (HFT stack
  pilot when it lands).
- **Custody and key management.** Different audience question.
- **KYC, AML, sanctions screening.** Upstream of this stack; they
  impose their own latency budgets but don't compete with the nine
  here.
- **Smart-contract layer.** Governance, fee accrual contracts, token
  issuance. Application code that lives on top of the price +
  position state this stack maintains.

The nine topics are the bones. Each topic page lists the recipes it
composes from; the recipes themselves carry the runnable code, the
sub-ms p99 assertion, and the non-claims for that primitive in
isolation.
