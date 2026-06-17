//! Resource semconv detection. Drives `SubMsOtelResource::detect()` under
//! controlled env, asserts attributes land. Env mutations are serialized via
//! a `Mutex` so the tests can run in any order without racing.

#![cfg(feature = "bridge")]

use std::sync::{Mutex, OnceLock};

use opentelemetry::KeyValue;
use subms_otel::SubMsOtelResource;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn clear_env() {
    for k in [
        "OTEL_SERVICE_NAME",
        "OTEL_SERVICE_VERSION",
        "OTEL_RESOURCE_ATTRIBUTES",
        "SUBMS_HOST",
        "SUBMS_HARDWARE_TIER",
        "AWS_REGION",
        "GCP_PROJECT",
        "GOOGLE_CLOUD_PROJECT",
        "AZURE_REGION",
    ] {
        unsafe {
            std::env::remove_var(k);
        }
    }
}

fn attrs_have(attrs: &[KeyValue], key: &str, expected: &str) -> bool {
    attrs
        .iter()
        .any(|kv| kv.key.as_str() == key && kv.value.as_str() == expected)
}

fn attrs_get(attrs: &[KeyValue], key: &str) -> Option<String> {
    attrs
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.as_str().into_owned())
}

#[test]
fn defaults_to_subms_service_name() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "service.name", "subms"));
    assert!(attrs_get(&r, "service.version").is_some());
}

#[test]
fn honors_otel_service_name_env() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("OTEL_SERVICE_NAME", "my-service");
    }
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "service.name", "my-service"));
    unsafe {
        std::env::remove_var("OTEL_SERVICE_NAME");
    }
}

#[test]
fn honors_subms_host_and_tier() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("SUBMS_HOST", "kr-laptop");
        std::env::set_var("SUBMS_HARDWARE_TIER", "laptop");
    }
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "subms.host", "kr-laptop"));
    assert!(attrs_have(&r, "subms.hardware.tier", "laptop"));
    assert!(attrs_have(&r, "host.name", "kr-laptop"));
    clear_env();
}

#[test]
fn parses_otel_resource_attributes_format() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var(
            "OTEL_RESOURCE_ATTRIBUTES",
            "deployment.environment=staging,tenant=acme",
        );
    }
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "deployment.environment", "staging"));
    assert!(attrs_have(&r, "tenant", "acme"));
    clear_env();
}

#[test]
fn detects_aws_when_region_set() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("AWS_REGION", "us-east-1");
    }
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "cloud.provider", "aws"));
    clear_env();
}

#[test]
fn populates_runtime_attrs() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    let r = SubMsOtelResource::detect();
    assert!(attrs_have(&r, "process.runtime.name", "rustc"));
    assert!(attrs_get(&r, "process.runtime.version").is_some());
    assert!(attrs_get(&r, "host.arch").is_some());
    assert!(attrs_get(&r, "os.type").is_some());
    assert!(attrs_get(&r, "service.instance.id").is_some());
}
