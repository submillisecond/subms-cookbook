use std::io::Write;
use std::process::{Command, Stdio};

use subms_ts::{TsCodec, TsJsonCodec, TsSeries};
use subms_ts_gzip::{TsGzipCodec, TsGzipCodecError, TsGzipError, gunzip, gzip};

fn series(points: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(t, v) in points {
        s.push(t, v).unwrap();
    }
    s
}

fn pairs(s: &TsSeries<f64>) -> Vec<(i64, f64)> {
    s.iter().map(|p| (p.ts, p.value)).collect()
}

fn repetitive(n: usize) -> TsSeries<f64> {
    // Highly compressible: a small repeating value cycle on a regular grid.
    let mut s = TsSeries::new();
    for i in 0..n as i64 {
        s.push(i * 10, (i % 8) as f64).unwrap();
    }
    s
}

fn codec() -> TsGzipCodec<TsJsonCodec, f64> {
    TsGzipCodec::new(TsJsonCodec::new(), 6)
}

#[test]
fn round_trip_basic() {
    let s = series(&[(1, 1.5), (2, 2.5), (3, 3.5)]);
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_empty() {
    let s = TsSeries::<f64>::new();
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    assert!(back.is_empty());
}

#[test]
fn round_trip_single_point() {
    let s = series(&[(42, 1.25)]);
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_negative_and_large_ts() {
    let s = series(&[(-1_000_000, -2.5), (0, 0.0), (9_000_000_000, 42.25)]);
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_many_points() {
    let mut state: u64 = 0xabcd_1234;
    let pts: Vec<(i64, f64)> = (0..5_000)
        .map(|i| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (i, (state >> 11) as f64 / 13.0 - 100.0)
        })
        .collect();
    let s = series(&pts);
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pts);
}

#[test]
fn values_are_bit_exact() {
    let s = series(&[(1, std::f64::consts::PI), (2, 1.0 / 3.0), (3, 1e-300)]);
    let c = codec();
    let back = c.decode(&c.encode(&s)).unwrap();
    for (a, b) in pairs(&s).iter().zip(pairs(&back)) {
        assert_eq!(a.1.to_bits(), b.1.to_bits());
    }
}

#[test]
fn compression_actually_shrinks() {
    let s = repetitive(5_000);
    let inner = TsJsonCodec::new();
    let raw = inner.encode(&s);
    let gz = codec().encode(&s);
    assert!(
        gz.len() < raw.len(),
        "gzip {} should be smaller than json {}",
        gz.len(),
        raw.len()
    );
    // A repetitive 5k series compresses well even on fixed-Huffman blocks;
    // 2x is a comfortable floor (we observe ~2.7x at level 6).
    assert!(gz.len() * 2 < raw.len(), "expected >2x on repetitive data");
}

#[test]
fn format_is_gzip_json() {
    assert_eq!(codec().format(), "gzip+json");
}

#[test]
fn levels_all_round_trip() {
    let s = repetitive(2_000);
    for level in 0..=9 {
        let c = TsGzipCodec::new(TsJsonCodec::new(), level);
        let back = c.decode(&c.encode(&s)).unwrap();
        assert_eq!(pairs(&back), pairs(&s), "level {level}");
    }
}

#[test]
fn stored_block_round_trips() {
    // level 0 forces a stored block; must still gunzip + decode cleanly.
    let s = series(&[(1, 1.0), (2, 2.0), (3, 3.0)]);
    let c = TsGzipCodec::new(TsJsonCodec::new(), 0);
    let bytes = c.encode(&s);
    let back = c.decode(&bytes).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn raw_gzip_gunzip_round_trip() {
    let payload = b"the quick brown fox jumps over the lazy dog, again and again and again";
    let gz = gzip(payload, 6);
    assert_eq!(&gz[..2], &[0x1f, 0x8b]);
    assert_eq!(gunzip(&gz).unwrap(), payload);
}

// ---- error cases ----

#[test]
fn decode_rejects_bad_magic() {
    let mut gz = codec().encode(&series(&[(1, 1.0)]));
    gz[0] = 0x00;
    let err = codec().decode(&gz).unwrap_err();
    assert_eq!(err, TsGzipCodecError::Gzip(TsGzipError::BadMagic));
}

#[test]
fn decode_rejects_truncated() {
    let gz = codec().encode(&series(&[(1, 1.0), (2, 2.0)]));
    let err = codec().decode(&gz[..gz.len() - 4]).unwrap_err();
    // A short trailer either trips Truncated or a CRC/size check; both are gzip-layer.
    assert!(matches!(err, TsGzipCodecError::Gzip(_)));
    let err2 = codec().decode(&[]).unwrap_err();
    assert_eq!(err2, TsGzipCodecError::Gzip(TsGzipError::Truncated));
}

#[test]
fn decode_rejects_corrupt_crc() {
    let mut gz = codec().encode(&repetitive(200));
    let n = gz.len();
    // Flip a CRC byte (trailer is the last 8 bytes; CRC is the first 4 of them).
    gz[n - 8] ^= 0xff;
    let err = codec().decode(&gz).unwrap_err();
    assert!(matches!(
        err,
        TsGzipCodecError::Gzip(TsGzipError::CrcMismatch { .. })
    ));
}

#[test]
fn decode_rejects_bad_method() {
    let mut gz = codec().encode(&series(&[(1, 1.0)]));
    gz[2] = 9; // CM=9, not deflate
    let err = codec().decode(&gz).unwrap_err();
    assert_eq!(err, TsGzipCodecError::Gzip(TsGzipError::BadMethod(9)));
}

// ---- interop: oracle is the system `gzip` / `gunzip` (gzip 1.14) ----

fn run_pipe(cmd: &str, args: &[&str], input: &[u8]) -> Option<Vec<u8>> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Feed stdin on a thread: a small pipe buffer can deadlock if we write the
    // whole input before draining stdout.
    let mut stdin = child.stdin.take()?;
    let buf = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&buf);
        drop(stdin);
    });
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    if out.status.success() {
        Some(out.stdout)
    } else {
        None
    }
}

// Oracle (a): our gzip output is decodable by the system tool. We pipe our
// encode() bytes through `gunzip -c` and assert the result equals the inner
// json encoding byte-for-byte.
#[test]
fn interop_our_output_gunzips() {
    let s = repetitive(5_000);
    let inner = TsJsonCodec::new();
    let expected = inner.encode(&s);
    let gz = codec().encode(&s);

    let decoded = tool_decompress(&gz).expect("system gunzip/gzip not available on PATH");
    assert_eq!(
        decoded, expected,
        "system gunzip of our output must equal inner json bytes"
    );
}

// Oracle (b): we can INFLATE the tool's output. We compress a known payload
// with the system `gzip` (dynamic-Huffman) and feed it to gunzip(); this
// exercises the dynamic-Huffman decode path.
#[test]
fn interop_we_inflate_tool_output() {
    let s = repetitive(5_000);
    let inner = TsJsonCodec::new();
    let payload = inner.encode(&s);

    let tool_gz = run_pipe("gzip", &["-9", "-c"], &payload)
        .expect("system gzip not available on PATH");
    let ours = gunzip(&tool_gz).expect("we must inflate the system gzip output");
    assert_eq!(ours, payload, "our gunzip of system gzip must round-trip");

    // And the full codec.decode path over the tool's gzip stream.
    let back = codec().decode(&tool_gz).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

// gunzip is an MSYS shell builtin on the build box and not always spawnable as
// a bare program; `gzip -d -c` is the portable decompress oracle.
fn tool_decompress(input: &[u8]) -> Option<Vec<u8>> {
    run_pipe("gunzip", &["-c"], input).or_else(|| run_pipe("gzip", &["-d", "-c"], input))
}

// Round-trip a small payload through the tool too, to cover short dynamic /
// stored choices the encoder makes at -9.
#[test]
fn interop_small_payload_both_ways() {
    let payload = b"abcabcabcabcabcabcabcabc-0123456789-abcabcabc";
    let tool_gz = run_pipe("gzip", &["-c"], payload).expect("system gzip");
    assert_eq!(gunzip(&tool_gz).unwrap(), payload);

    let ours = gzip(payload, 9);
    let back = tool_decompress(&ours).expect("system gzip -d");
    assert_eq!(back, payload);
}
