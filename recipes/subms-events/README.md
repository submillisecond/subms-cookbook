# subms-events

A [submillisecond.com](https://www.submillisecond.com/cookbook/recipes/subms-events)
cookbook recipe - `concurrency`.

A low-latency in-process event system: a structured `Event` + builder, a
sync-inline / async-off-thread dispatcher, composable fn / composite / filter
listeners, and an `EventBridge` sink interface that adapters implement.

The substrate that [subms-health](https://www.submillisecond.com/cookbook/recipes/subms-health)
emits status changes through and that
[subms-otel](https://www.submillisecond.com/cookbook/recipes/subms-otel) bridges
to OTEL. Reference languages: Rust, Python, Java (byte-identical `Event` JSON).

## Install

```toml
# Cargo.toml
subms-events = "0.9"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-events</artifactId>
  <version>0.9.1</version>
</dependency>
```

- **Rust:** `cargo add subms-events`
- **Python:** `pip install subms-events`
- **Java:** `com.submillisecond.recipes:subms-events`

Sync dispatch runs listeners inline (no thread, no allocation beyond the event);
async dispatch hands the event to a daemon thread over a queue, so a slow
listener never blocks the producer.

Licensed under MIT OR Apache-2.0.
