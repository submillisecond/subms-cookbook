//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe};

use crate::{SegmentReader, SegmentWriter};

pub struct SegmentReaderRecipe;

impl SubMsRecipe for SegmentReaderRecipe {
    fn name(&self) -> &str {
        "segment-reader"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        // Build the segment first; the bench measures read latency only.
        let mut buf: Vec<u8> = Vec::with_capacity(entries * 32);
        {
            let mut w = SegmentWriter::new(&mut buf);
            for i in 0..entries {
                let record = format!("record-{i}");
                w.write(record.as_bytes()).expect("write");
            }
        }

        let s = h.stage("next_record", entries);
        let mut r = SegmentReader::new(buf.as_slice());
        for _ in 0..entries {
            let t0 = Instant::now();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("segment_bytes", &buf.len().to_string());
    }
}
