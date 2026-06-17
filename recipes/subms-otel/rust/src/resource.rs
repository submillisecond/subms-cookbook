//! Resource semconv detector. Surfaces the `service.*`, `host.*`, `os.*`,
//! `process.runtime.*`, `subms.*`, and `cloud.provider` attributes the
//! OpenTelemetry spec calls out. Returns a plain `Vec<KeyValue>` so the
//! bridge stays SDK-free; callers wiring an SDK Resource do
//! `Resource::builder_empty().with_attributes(detect()).build()`.

use std::env;

use opentelemetry::KeyValue;

const DEFAULT_SERVICE_NAME: &str = "subms";

/// Static helper carrying the detected resource set.
pub struct SubMsOtelResource;

impl SubMsOtelResource {
    /// Detect the resource attributes from environment + runtime introspection.
    /// Honors `OTEL_SERVICE_NAME`, `OTEL_SERVICE_VERSION`,
    /// `OTEL_RESOURCE_ATTRIBUTES`, `SUBMS_HOST`, `SUBMS_HARDWARE_TIER`,
    /// and the standard cloud-provider env probes.
    pub fn detect() -> Vec<KeyValue> {
        let mut kvs: Vec<KeyValue> = Vec::new();

        let service_name = env::var("OTEL_SERVICE_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_string());
        kvs.push(KeyValue::new("service.name", service_name));

        let service_version = env::var("OTEL_SERVICE_VERSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        kvs.push(KeyValue::new("service.version", service_version));

        kvs.push(KeyValue::new("service.instance.id", random_instance_id()));

        if let Some(host) = detect_host_name() {
            kvs.push(KeyValue::new("host.name", host));
        }
        kvs.push(KeyValue::new("host.arch", std::env::consts::ARCH));
        kvs.push(KeyValue::new("os.type", std::env::consts::OS));
        if let Some(v) = os_version() {
            kvs.push(KeyValue::new("os.version", v));
        }

        kvs.push(KeyValue::new("process.runtime.name", "rustc"));
        kvs.push(KeyValue::new(
            "process.runtime.version",
            env!("CARGO_PKG_RUST_VERSION"),
        ));

        if let Ok(v) = env::var("SUBMS_HOST")
            && !v.is_empty()
        {
            kvs.push(KeyValue::new("subms.host", v));
        }
        if let Ok(v) = env::var("SUBMS_HARDWARE_TIER")
            && !v.is_empty()
        {
            kvs.push(KeyValue::new("subms.hardware.tier", v));
        }

        if env::var("AWS_REGION").is_ok() {
            kvs.push(KeyValue::new("cloud.provider", "aws"));
        } else if env::var("GCP_PROJECT").is_ok() || env::var("GOOGLE_CLOUD_PROJECT").is_ok() {
            kvs.push(KeyValue::new("cloud.provider", "gcp"));
        } else if env::var("AZURE_REGION").is_ok() {
            kvs.push(KeyValue::new("cloud.provider", "azure"));
        }

        if let Ok(extra) = env::var("OTEL_RESOURCE_ATTRIBUTES") {
            for entry in extra.split(',') {
                if let Some((k, v)) = entry.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    if !k.is_empty() && !v.is_empty() {
                        kvs.push(KeyValue::new(k.to_string(), v.to_string()));
                    }
                }
            }
        }

        kvs
    }
}

fn detect_host_name() -> Option<String> {
    if let Ok(v) = env::var("SUBMS_HOST")
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Ok(v) = env::var("HOSTNAME")
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Ok(v) = env::var("COMPUTERNAME")
        && !v.is_empty()
    {
        return Some(v);
    }
    None
}

fn os_version() -> Option<String> {
    env::var("OS_VERSION").ok().filter(|s| !s.is_empty())
}

fn random_instance_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mixed = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(pid);
    format!("{:032x}", mixed)
}
