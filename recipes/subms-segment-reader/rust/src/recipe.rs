//! `SubMsRecipe` impl.

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

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

        let s = h
            .stage("next_record", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut r = SegmentReader::new(buf.as_slice());
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let _ = r.next_record().expect("read");
            s.record(t0.elapsed_ns());
        }

        h.add_meta("segment_bytes", &buf.len().to_string());
    }
}
