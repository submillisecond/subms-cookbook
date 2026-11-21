//! A minimal server-sent-events writer and reader: `data:` lines, comments,
//! CRLF or LF endings, dispatch on a blank line. `event:`, `id:` and `retry:`
//! are skipped rather than surfaced; nothing here needs them.
//!
//! The writer formats a whole event into one buffer and issues one
//! `write_all`. Three writes per event would be three syscalls, and with
//! `TCP_NODELAY` possibly three segments, which would price the writer rather
//! than the format.

use std::io::{self, BufRead, Write};

/// The response head an SSE endpoint sends before the first event. No
/// `Transfer-Encoding: chunked`: the body is delimited by connection close,
/// which HTTP/1.1 allows and which keeps chunk framing out of the
/// measurement.
pub const RESPONSE_HEAD: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream\r\n\
Cache-Control: no-cache\r\n\
Connection: keep-alive\r\n\r\n";

/// The request a client sends to open the stream.
pub const REQUEST_HEAD: &[u8] = b"GET /stream HTTP/1.1\r\n\
Host: 127.0.0.1\r\n\
Accept: text/event-stream\r\n\r\n";

/// Formats events into a reused buffer and writes each with one call.
pub struct SseWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: Write> SseWriter<W> {
    pub fn new(inner: W) -> Self {
        SseWriter {
            inner,
            buf: Vec::with_capacity(256),
        }
    }

    /// Write one event. A `\n` inside `data` becomes a second `data:` line,
    /// which the reader joins back with `\n`, per the spec.
    pub fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.buf.clear();
        for line in data.split(|b| *b == b'\n') {
            self.buf.extend_from_slice(b"data: ");
            self.buf.extend_from_slice(line);
            self.buf.push(b'\n');
        }
        self.buf.push(b'\n');
        self.inner.write_all(&self.buf)
    }

    /// A comment line. Proxies and clients ignore it; servers send these as
    /// keep-alives.
    pub fn comment(&mut self, text: &[u8]) -> io::Result<()> {
        self.buf.clear();
        self.buf.push(b':');
        self.buf.extend_from_slice(text);
        self.buf.extend_from_slice(b"\n");
        self.inner.write_all(&self.buf)
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

/// Parses events off a buffered stream.
pub struct SseReader<R: BufRead> {
    inner: R,
    line: Vec<u8>,
}

impl<R: BufRead> SseReader<R> {
    pub fn new(inner: R) -> Self {
        SseReader {
            inner,
            line: Vec::with_capacity(256),
        }
    }

    /// Read the next event's data into `out` (cleared first). `Ok(false)` at
    /// end of stream. An event with no `data:` lines is skipped, as a browser
    /// would.
    pub fn next_event(&mut self, out: &mut Vec<u8>) -> io::Result<bool> {
        out.clear();
        let mut have = false;
        loop {
            self.line.clear();
            if self.inner.read_until(b'\n', &mut self.line)? == 0 {
                return Ok(false);
            }
            let mut l: &[u8] = &self.line;
            if let [rest @ .., b'\n'] = l {
                l = rest;
            }
            if let [rest @ .., b'\r'] = l {
                l = rest;
            }
            if l.is_empty() {
                if have {
                    out.pop();
                    return Ok(true);
                }
                continue;
            }
            if l[0] == b':' {
                continue;
            }
            if let Some(v) = l.strip_prefix(b"data:") {
                let v = v.strip_prefix(b" ").unwrap_or(v);
                out.extend_from_slice(v);
                out.push(b'\n');
                have = true;
            }
        }
    }

    /// Consume an HTTP head up to and including the blank line.
    pub fn skip_http_head(&mut self) -> io::Result<()> {
        skip_head(&mut self.inner, &mut self.line)
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

/// Consume an HTTP head (request or response) up to the blank line.
pub fn skip_head<R: BufRead>(r: &mut R, scratch: &mut Vec<u8>) -> io::Result<()> {
    loop {
        scratch.clear();
        if r.read_until(b'\n', scratch)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "stream closed inside the HTTP head",
            ));
        }
        if scratch == b"\r\n" || scratch == b"\n" {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
