//! Hand-rolled in-memory exporters used only by the integration test suite.
//! Replaces `opentelemetry_sdk::testing::*` so the dev-dep no longer needs
//! the `testing` feature.
//!
//! `ResourceMetrics` is `pub(crate)`-fielded, so we cannot clone it out of
//! the SDK. Instead, on each `export(...)` we project the metrics into a
//! `Vec<MetricSnapshot>` of plain, public-fielded structs and stash those.
//! Tests read snapshots rather than walking SDK types.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::Temporality;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::trace::{SpanData, SpanExporter};

#[derive(Debug, Clone)]
pub struct MetricSnapshot {
    pub name: String,
    pub unit: String,
    pub points: Vec<PointSnapshot>,
}

#[derive(Debug, Clone)]
pub enum PointSnapshot {
    HistogramF64 {
        attributes: Vec<KeyValue>,
        count: u64,
        sum: f64,
        bucket_counts: Vec<u64>,
        bounds: Vec<f64>,
    },
    HistogramU64 {
        attributes: Vec<KeyValue>,
        count: u64,
        sum: u64,
    },
    SumU64 {
        attributes: Vec<KeyValue>,
        value: u64,
    },
    SumI64 {
        attributes: Vec<KeyValue>,
        value: i64,
    },
    SumF64 {
        attributes: Vec<KeyValue>,
        value: f64,
    },
    GaugeU64 {
        attributes: Vec<KeyValue>,
        value: u64,
    },
    GaugeI64 {
        attributes: Vec<KeyValue>,
        value: i64,
    },
    GaugeF64 {
        attributes: Vec<KeyValue>,
        value: f64,
    },
}

impl PointSnapshot {
    pub fn attributes(&self) -> &[KeyValue] {
        match self {
            Self::HistogramF64 { attributes, .. }
            | Self::HistogramU64 { attributes, .. }
            | Self::SumU64 { attributes, .. }
            | Self::SumI64 { attributes, .. }
            | Self::SumF64 { attributes, .. }
            | Self::GaugeU64 { attributes, .. }
            | Self::GaugeI64 { attributes, .. }
            | Self::GaugeF64 { attributes, .. } => attributes,
        }
    }

    pub fn histogram_count(&self) -> Option<u64> {
        match self {
            Self::HistogramF64 { count, .. } | Self::HistogramU64 { count, .. } => Some(*count),
            _ => None,
        }
    }

    pub fn sum_u64(&self) -> Option<u64> {
        match self {
            Self::SumU64 { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn gauge_u64(&self) -> Option<u64> {
        match self {
            Self::GaugeU64 { value, .. } => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryMetricExporter {
    snapshots: Arc<Mutex<Vec<Vec<MetricSnapshot>>>>,
}

impl InMemoryMetricExporter {
    pub fn get_finished_metrics(&self) -> Result<Vec<Vec<MetricSnapshot>>, &'static str> {
        let guard = self.snapshots.lock().map_err(|_| "poisoned")?;
        Ok(guard.clone())
    }

    pub fn reset(&self) {
        if let Ok(mut g) = self.snapshots.lock() {
            g.clear();
        }
    }
}

impl PushMetricExporter for InMemoryMetricExporter {
    async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let snapshot = project_resource_metrics(metrics);
        let mut guard = self.snapshots.lock().expect("in-mem metric poisoned");
        guard.push(snapshot);
        Ok(())
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

fn project_resource_metrics(metrics: &ResourceMetrics) -> Vec<MetricSnapshot> {
    let mut out = Vec::new();
    for sm in metrics.scope_metrics() {
        for m in sm.metrics() {
            let mut points = Vec::new();
            match m.data() {
                AggregatedMetrics::F64(MetricData::Histogram(h)) => {
                    for dp in h.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        let bucket_counts: Vec<u64> = dp.bucket_counts().collect();
                        let bounds: Vec<f64> = dp.bounds().collect();
                        points.push(PointSnapshot::HistogramF64 {
                            attributes,
                            count: dp.count(),
                            sum: dp.sum(),
                            bucket_counts,
                            bounds,
                        });
                    }
                }
                AggregatedMetrics::U64(MetricData::Histogram(h)) => {
                    for dp in h.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::HistogramU64 {
                            attributes,
                            count: dp.count(),
                            sum: dp.sum(),
                        });
                    }
                }
                AggregatedMetrics::U64(MetricData::Sum(s)) => {
                    for dp in s.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::SumU64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                AggregatedMetrics::I64(MetricData::Sum(s)) => {
                    for dp in s.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::SumI64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                AggregatedMetrics::F64(MetricData::Sum(s)) => {
                    for dp in s.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::SumF64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                AggregatedMetrics::U64(MetricData::Gauge(g)) => {
                    for dp in g.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::GaugeU64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                AggregatedMetrics::I64(MetricData::Gauge(g)) => {
                    for dp in g.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::GaugeI64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                AggregatedMetrics::F64(MetricData::Gauge(g)) => {
                    for dp in g.data_points() {
                        let attributes: Vec<KeyValue> = dp.attributes().cloned().collect();
                        points.push(PointSnapshot::GaugeF64 {
                            attributes,
                            value: dp.value(),
                        });
                    }
                }
                _ => {}
            }
            out.push(MetricSnapshot {
                name: m.name().to_string(),
                unit: m.unit().to_string(),
                points,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySpanExporter {
    finished: Arc<Mutex<Vec<SpanData>>>,
}

impl InMemorySpanExporter {
    pub fn get_finished_spans(&self) -> Result<Vec<SpanData>, &'static str> {
        let guard = self.finished.lock().map_err(|_| "poisoned")?;
        Ok(guard.clone())
    }

    pub fn reset(&self) {
        if let Ok(mut g) = self.finished.lock() {
            g.clear();
        }
    }
}

impl SpanExporter for InMemorySpanExporter {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let mut guard = self.finished.lock().expect("in-mem span poisoned");
        guard.extend(batch);
        Ok(())
    }
}

pub fn flatten_metric_snapshots(
    snapshots: &[Vec<MetricSnapshot>],
) -> impl Iterator<Item = &MetricSnapshot> {
    snapshots.iter().flat_map(|s| s.iter())
}

pub fn sum_counter(snapshots: &[Vec<MetricSnapshot>], name: &str) -> u64 {
    let mut total = 0u64;
    for snap in snapshots {
        for m in snap {
            if m.name == name {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        total += *value;
                    }
                }
            }
        }
    }
    total
}
