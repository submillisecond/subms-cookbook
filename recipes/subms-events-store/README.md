# subms-events-store

A [submillisecond.com](https://www.submillisecond.com/cookbook/recipes/subms-events-store)
cookbook recipe - `storage`.

In-memory event sourcing on
[subms-events](https://www.submillisecond.com/cookbook/recipes/subms-events): an
append-only log with offset addressing, full replay, incremental projections
(`catch_up` applies only the tail), and live subscriptions. Reference languages:
Rust, Python, Java (byte-identical log JSON across all three).

## Install

```toml
# Cargo.toml
subms-events-store = "0.9"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-events-store</artifactId>
  <version>0.9.1</version>
</dependency>
```

- **Rust:** `cargo add subms-events-store`
- **Python:** `pip install subms-events-store`
- **Java:** `com.submillisecond.recipes:subms-events-store`

The log is in-memory. Durability is the consumer's - pair with
[subms-ts-wal](https://www.submillisecond.com/cookbook/recipes/subms-ts-wal) to
persist the append stream.

Licensed under MIT OR Apache-2.0.
