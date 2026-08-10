# subms-health

A [submillisecond.com](https://www.submillisecond.com/cookbook/recipes/subms-health)
cookbook recipe - `observability`.

A health endpoint library: indicators with worst-wins aggregation, a
redaction-aware env/deploy provider, Kubernetes probe kinds, critical /
non-critical demotion, and a background-refreshed cached snapshot that serves
`/health` in sub-microsecond time. The request path never probes a dependency.

Built on [subms-events](https://www.submillisecond.com/cookbook/recipes/subms-events).
Reference languages: Rust, Python, Java (byte-identical JSON across all three).

## Install

```toml
# Cargo.toml
subms-health = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-health</artifactId>
  <version>0.10.0</version>
</dependency>
```

- **Rust:** `cargo add subms-health`
- **Python:** `pip install subms-health`
- **Java:** `com.submillisecond.recipes:subms-health`

Statuses: `UP < UNKNOWN < WARN < DEGRADED < DOWN` (worst-wins). HTTP codes are
probe-aware - liveness only 503s on `DOWN`, so a `DEGRADED` process is never
restarted. See the writeup for the full table and the quality bar.

Licensed under MIT OR Apache-2.0.
