//! Pluggable HTTP transport. The adapter builds requests; a transport ships
//! the bytes. The default `StdHttpTransport` is a zero-dep HTTP/1.1 client over
//! `std::net::TcpStream`; tests inject a `CaptureTransport` so the request
//! construction + response decoding are exercised without a live server.

use crate::error::TsInfluxError;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpStream;

#[derive(Clone, Debug, PartialEq)]
pub struct TsHttpRequest {
    pub method: String,
    /// Path plus query string, e.g. `/api/v2/write?org=o&bucket=b&precision=ns`.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TsHttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait TsInfluxTransport {
    fn send(&self, req: &TsHttpRequest) -> Result<TsHttpResponse, TsInfluxError>;
}

/// Zero-dep HTTP/1.1 client. Plaintext only - TLS would pull a crypto dep, so
/// for an `https` endpoint front it with a TLS-terminating proxy (stated
/// non-claim on the recipe page).
pub struct StdHttpTransport {
    host: String,
    port: u16,
}

impl StdHttpTransport {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

impl TsInfluxTransport for StdHttpTransport {
    fn send(&self, req: &TsHttpRequest) -> Result<TsHttpResponse, TsInfluxError> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| TsInfluxError::transport(e.to_string()))?;

        let mut wire = format!("{} {} HTTP/1.1\r\n", req.method, req.path);
        wire.push_str(&format!("Host: {}:{}\r\n", self.host, self.port));
        wire.push_str("Connection: close\r\n");
        let mut has_len = false;
        for (k, v) in &req.headers {
            if k.eq_ignore_ascii_case("content-length") {
                has_len = true;
            }
            wire.push_str(&format!("{k}: {v}\r\n"));
        }
        if !has_len {
            wire.push_str(&format!("Content-Length: {}\r\n", req.body.len()));
        }
        wire.push_str("\r\n");
        wire.push_str(&req.body);

        stream
            .write_all(wire.as_bytes())
            .map_err(|e| TsInfluxError::transport(e.to_string()))?;

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| TsInfluxError::transport(e.to_string()))?;
        parse_http_response(&raw)
    }
}

fn parse_http_response(raw: &[u8]) -> Result<TsHttpResponse, TsInfluxError> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| TsInfluxError::transport("no header terminator"))?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let body = String::from_utf8_lossy(&raw[split + 4..]).into_owned();
    let status_line = head.lines().next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| TsInfluxError::transport("no status code"))?;
    Ok(TsHttpResponse { status, body })
}

/// Records every request and replays a queued response. Test-only injection
/// point - keeps the adapter's request shaping under unit test without a
/// network.
pub struct CaptureTransport {
    pub sent: RefCell<Vec<TsHttpRequest>>,
    responses: RefCell<Vec<TsHttpResponse>>,
}

impl CaptureTransport {
    pub fn new(responses: Vec<TsHttpResponse>) -> Self {
        Self {
            sent: RefCell::new(Vec::new()),
            responses: RefCell::new(responses),
        }
    }
    pub fn ok(body: &str) -> Self {
        Self::new(vec![TsHttpResponse {
            status: 200,
            body: body.to_string(),
        }])
    }
    pub fn last_body(&self) -> Option<String> {
        self.sent.borrow().last().map(|r| r.body.clone())
    }
}

impl TsInfluxTransport for CaptureTransport {
    fn send(&self, req: &TsHttpRequest) -> Result<TsHttpResponse, TsInfluxError> {
        self.sent.borrow_mut().push(req.clone());
        let mut r = self.responses.borrow_mut();
        if r.is_empty() {
            Ok(TsHttpResponse {
                status: 204,
                body: String::new(),
            })
        } else {
            Ok(r.remove(0))
        }
    }
}
