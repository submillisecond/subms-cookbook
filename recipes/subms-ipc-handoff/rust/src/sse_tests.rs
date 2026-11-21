use super::*;
use std::io::Cursor;

#[test]
fn writer_formats_one_event_per_call() {
    let mut w = SseWriter::new(Vec::new());
    w.send(b"hello").unwrap();
    w.send(b"two\nlines").unwrap();
    w.comment(b" keep-alive").unwrap();
    assert_eq!(
        w.get_mut().as_slice(),
        b"data: hello\n\ndata: two\ndata: lines\n\n: keep-alive\n".as_slice()
    );
}

#[test]
fn reader_round_trips_the_writer() {
    let mut w = SseWriter::new(Vec::new());
    w.send(b"a").unwrap();
    w.comment(b"ignored").unwrap();
    w.send(b"b\nc").unwrap();
    let bytes = std::mem::take(w.get_mut());
    let mut r = SseReader::new(Cursor::new(bytes));
    let mut out = Vec::new();
    assert!(r.next_event(&mut out).unwrap());
    assert_eq!(out, b"a");
    assert!(r.next_event(&mut out).unwrap());
    assert_eq!(out, b"b\nc");
    assert!(!r.next_event(&mut out).unwrap());
}

#[test]
fn reader_handles_crlf_no_space_and_other_fields() {
    let src = b"event: tick\r\nid: 7\r\ndata:x\r\n\r\n\r\nretry: 10\n\ndata: y\n\n";
    let mut r = SseReader::new(Cursor::new(src.to_vec()));
    let mut out = Vec::new();
    assert!(r.next_event(&mut out).unwrap());
    assert_eq!(out, b"x");
    assert!(r.next_event(&mut out).unwrap());
    assert_eq!(out, b"y");
    assert!(!r.next_event(&mut out).unwrap());
}

#[test]
fn http_heads_are_skipped() {
    let mut src = RESPONSE_HEAD.to_vec();
    src.extend_from_slice(b"data: z\n\n");
    let mut r = SseReader::new(Cursor::new(src));
    r.skip_http_head().unwrap();
    let mut out = Vec::new();
    assert!(r.next_event(&mut out).unwrap());
    assert_eq!(out, b"z");
    assert_eq!(r.get_mut().position() as usize, RESPONSE_HEAD.len() + 9);

    let mut req = Cursor::new(REQUEST_HEAD.to_vec());
    skip_head(&mut req, &mut Vec::new()).unwrap();
    assert_eq!(req.position() as usize, REQUEST_HEAD.len());
}

#[test]
fn truncated_head_is_an_error() {
    let mut r = SseReader::new(Cursor::new(b"HTTP/1.1 200 OK\r\n".to_vec()));
    let e = r.skip_http_head().unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof);
}
