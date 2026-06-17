//! `subms-ts-adapter-influxdb` - a zero-dep InfluxDB v2 adapter for the submillisecond
//! cookbook timeseries arc.
//!
//! Two pure cores - a line-protocol encoder ([`line`]) and an annotated-CSV
//! decoder ([`csv`]) - sit behind an injectable [`TsInfluxTransport`]. The
//! default transport is a hand-rolled HTTP/1.1 client over `std::net`; tests
//! inject a capture transport, so request shaping and response decoding are
//! fully exercised without a live database.
//!
//! Mapping: each `TsSeries<f64>` is one Influx measurement (named by its
//! metadata, or an explicit override), its metadata tags become the Influx tag
//! set, and the point value is a single field `v`. Timestamps are nanoseconds.

mod csv;
mod error;
mod line;
mod time;
mod transport;

#[cfg(feature = "harness")]
pub mod recipe;

pub use csv::decode_response;
pub use error::TsInfluxError;
pub use line::{encode_collection, encode_line, encode_series};
pub use time::{format_rfc3339_nanos, parse_rfc3339_nanos};
pub use transport::{
    CaptureTransport, StdHttpTransport, TsHttpRequest, TsHttpResponse, TsInfluxTransport,
};

use subms_ts::{TsCollection, TsSeries};

/// InfluxDB v2 adapter parameterised over a transport.
pub struct TsInfluxAdapter<X: TsInfluxTransport> {
    transport: X,
    org: String,
    bucket: String,
    token: String,
}

impl TsInfluxAdapter<StdHttpTransport> {
    /// Connect to a plaintext `http://host[:port]` InfluxDB v2 endpoint. An
    /// `https://` URL is rejected - front it with a TLS-terminating proxy.
    pub fn connect(
        url: &str,
        token: impl Into<String>,
        org: impl Into<String>,
        bucket: impl Into<String>,
    ) -> Result<Self, TsInfluxError> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| TsInfluxError::config("only http:// endpoints are supported"))?;
        let authority = rest.split('/').next().unwrap_or(rest);
        let (host, port) = match authority.split_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse().map_err(|_| TsInfluxError::config("bad port"))?,
            ),
            None => (authority.to_string(), 8086u16),
        };
        if host.is_empty() {
            return Err(TsInfluxError::config("empty host"));
        }
        Ok(Self {
            transport: StdHttpTransport::new(host, port),
            org: org.into(),
            bucket: bucket.into(),
            token: token.into(),
        })
    }
}

impl<X: TsInfluxTransport> TsInfluxAdapter<X> {
    /// Build over a caller-supplied transport (the injection point for tests
    /// and for alternative HTTP stacks).
    pub fn with_transport(
        transport: X,
        token: impl Into<String>,
        org: impl Into<String>,
        bucket: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            org: org.into(),
            bucket: bucket.into(),
            token: token.into(),
        }
    }

    /// Borrow the underlying transport (the inspection point for a
    /// `CaptureTransport` under test).
    pub fn transport(&self) -> &X {
        &self.transport
    }

    /// Line-protocol write of one series. `measurement` overrides the series
    /// metadata name when non-empty. Returns the number of points written.
    pub fn write_series(
        &self,
        series: &TsSeries<f64>,
        measurement: &str,
    ) -> Result<usize, TsInfluxError> {
        let body = encode_series(series, measurement);
        self.write_body(body, series.len())
    }

    /// Line-protocol write of every series in a collection.
    pub fn write_collection(&self, coll: &TsCollection<f64>) -> Result<usize, TsInfluxError> {
        let body = encode_collection(coll);
        let n: usize = coll.series().map(|s| s.len()).sum();
        self.write_body(body, n)
    }

    fn write_body(&self, body: String, npoints: usize) -> Result<usize, TsInfluxError> {
        if body.is_empty() {
            return Ok(0);
        }
        let path = format!(
            "/api/v2/write?org={}&bucket={}&precision=ns",
            pct(&self.org),
            pct(&self.bucket)
        );
        let req = TsHttpRequest {
            method: "POST".into(),
            path,
            headers: vec![
                ("Authorization".into(), format!("Token {}", self.token)),
                ("Content-Type".into(), "text/plain; charset=utf-8".into()),
            ],
            body,
        };
        let resp = self.transport.send(&req)?;
        if (200..300).contains(&resp.status) {
            Ok(npoints)
        } else {
            Err(TsInfluxError::Http {
                status: resp.status,
                body: resp.body,
            })
        }
    }

    /// Run a Flux query and decode the annotated-CSV response into a collection.
    pub fn query_flux(&self, flux: &str) -> Result<TsCollection<f64>, TsInfluxError> {
        let path = format!("/api/v2/query?org={}", pct(&self.org));
        let req = TsHttpRequest {
            method: "POST".into(),
            path,
            headers: vec![
                ("Authorization".into(), format!("Token {}", self.token)),
                ("Content-Type".into(), "application/vnd.flux".into()),
                ("Accept".into(), "application/csv".into()),
            ],
            body: flux.to_string(),
        };
        let resp = self.transport.send(&req)?;
        if !(200..300).contains(&resp.status) {
            return Err(TsInfluxError::Http {
                status: resp.status,
                body: resp.body,
            });
        }
        decode_response(&resp.body)
    }
}

/// Percent-encode a query-string value (RFC3986 unreserved set passes through).
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
