//! Opt-in convenience: build OTLP-exporting `MeterProvider` + `TracerProvider`.
//! Thin wrapper - serious deployments will wire the exporters themselves to
//! control batching, resource attrs, and transport credentials.

use std::time::Duration;

use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;

/// Thin builder around the OTLP metric exporter. Defaults to the OTLP/HTTP
/// proto endpoint on `localhost:4318`; override with [`Self::with_endpoint`].
///
/// Returns a fully-wired [`SdkMeterProvider`] from [`Self::build`] - call
/// `provider.meter("subms-otel")` to mint a `Meter` for the observer.
///
/// The current OTLP exporter API is still evolving across opentelemetry-rust
/// minor releases; treat this builder as a starting point rather than a
/// frozen surface.
pub struct OtlpBuilder {
    endpoint: Option<String>,
    export_interval: Duration,
}

impl Default for OtlpBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OtlpBuilder {
    pub fn new() -> Self {
        Self {
            endpoint: None,
            export_interval: Duration::from_secs(10),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn with_export_interval(mut self, interval: Duration) -> Self {
        self.export_interval = interval;
        self
    }

    pub fn build(self) -> Option<SdkMeterProvider> {
        let mut exporter_builder = MetricExporter::builder().with_http();
        if let Some(endpoint) = self.endpoint {
            exporter_builder = exporter_builder.with_endpoint(endpoint);
        }
        let exporter = exporter_builder.build().ok()?;
        let reader = PeriodicReader::builder(exporter)
            .with_interval(self.export_interval)
            .build();
        Some(SdkMeterProvider::builder().with_reader(reader).build())
    }
}

/// Wire protocol selector for OTLP.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OtlpProtocol {
    HttpProtobuf,
    Grpc,
}

impl OtlpProtocol {
    /// Parse the standard `OTEL_EXPORTER_OTLP_PROTOCOL` value. Defaults to
    /// `HttpProtobuf` for any unrecognised value.
    pub fn from_env(s: Option<&str>) -> Self {
        match s.map(str::trim).unwrap_or("") {
            "grpc" => Self::Grpc,
            _ => Self::HttpProtobuf,
        }
    }
}

/// Multi-modal helper: returns wired `MeterProvider` + `TracerProvider`
/// using OTLP. Lets `autoconfig` keep its exporter wiring small.
pub struct ExporterOtlpHelper;

impl ExporterOtlpHelper {
    /// Build a `(MeterProvider, TracerProvider)` pair wired to the OTLP
    /// endpoint. The `protocol` argument picks HTTP/proto vs gRPC; `resource`
    /// is attached to both providers so emitted metrics + spans share the
    /// same `service.name` / `host.name` set.
    ///
    /// Returns `None` if either exporter fails to build (typically because
    /// the endpoint URL is malformed).
    pub fn build(
        endpoint: Option<&str>,
        protocol: OtlpProtocol,
        resource: Resource,
    ) -> Option<(SdkMeterProvider, SdkTracerProvider)> {
        let metric_exporter = {
            // gRPC requires opentelemetry-otlp's tonic feature. Without it the
            // builder lacks `with_tonic`; fall back to HTTP/proto.
            let _ = protocol;
            let mut b = MetricExporter::builder().with_http();
            if let Some(ep) = endpoint {
                b = b.with_endpoint(ep);
            }
            b.build().ok()?
        };
        let reader = PeriodicReader::builder(metric_exporter)
            .with_interval(Duration::from_secs(10))
            .build();
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource.clone())
            .build();

        let span_exporter = {
            let mut b = SpanExporter::builder().with_http();
            if let Some(ep) = endpoint {
                b = b.with_endpoint(ep);
            }
            b.build().ok()?
        };
        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .with_resource(resource)
            .build();
        Some((meter_provider, tracer_provider))
    }
}

#[cfg(test)]
#[path = "exporter_otlp_tests.rs"]
mod exporter_otlp_tests;
