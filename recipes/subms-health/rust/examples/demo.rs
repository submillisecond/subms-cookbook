//! Zero-dep demo: a registry with a couple of indicators + a redacted deploy
//! section, rendered to stdout. Run: `cargo run --example demo`.
//!
//! HTTP wiring is a few lines on top - with axum:
//!
//! ```ignore
//! let reg = Arc::new(reg);
//! let app = Router::new().route("/health", get({
//!     let reg = reg.clone();
//!     move || async move {
//!         let (code, body) = reg.render();
//!         (StatusCode::from_u16(code).unwrap(), body)
//!     }
//! }));
//! ```

use std::sync::Arc;

use subms_health::{ComponentHealth, EnvSection, HealthRegistry, MapEnv, RefreshPolicy};

fn main() {
    let mut reg = HealthRegistry::with_system_sections();

    reg.register_fn("db", RefreshPolicy::new(), || {
        ComponentHealth::up()
            .with_detail("ping", "ok")
            .with_detail("pool_idle", 7i64)
    });
    reg.register_fn("cache", RefreshPolicy::new().critical(false), || {
        ComponentHealth::down("connection refused")
    });

    let env = MapEnv::new()
        .with("KICKSTART_ENV", "prod")
        .with("KICKSTART_VERSION", "1.4.2")
        .with("KICKSTART_TOKEN", "hunter2-do-not-log");
    let section = EnvSection::new("deploy-extra")
        .prefix("KICKSTART_")
        .strip_prefix_in_key(true)
        .lowercase_keys(true)
        .redact_secrets();
    reg.register(
        Arc::new(section.into_indicator(Arc::new(env))),
        RefreshPolicy::new().critical(false),
    );

    let (code, body) = reg.render();
    println!("HTTP {code}");
    println!("{body}");

    let (live_code, _) = reg.render_liveness();
    println!("\n/health/live -> HTTP {live_code}");
}
