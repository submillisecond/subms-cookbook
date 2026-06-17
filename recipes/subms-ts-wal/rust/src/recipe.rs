//! `SubMsRecipe` impl - measure the WAL append hot path under the two batched
//! fsync policies the sub-ms claim covers. The fsync-per-append (`Always`)
//! figure is deliberately NOT asserted here; it is fsync-floor-limited and
//! measured separately in the writeup.

use std::time::{SystemTime, UNIX_EPOCH};

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{TsFsyncPolicy, TsWal};

pub struct WalRecipe;

impl WalRecipe {
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "subms-ts-wal-bench-{}-{tag}-{nanos}",
            std::process::id()
        ))
    }

    fn drive(h: &mut SubMsPerfHarness, stage: &str, rounds: usize, policy: TsFsyncPolicy) {
        let dir = Self::scratch_dir(stage);
        let mut wal = TsWal::open(&dir, policy).expect("open wal");
        let s = h.stage(stage, rounds).with_kind(SubMsStageKind::HotPath);
        for i in 0..rounds {
            let ts = i as i64;
            let value = (i as f64) * 0.5;
            let t0 = SubMsTimer::tick();
            wal.append(7, ts, value).expect("append");
            s.record(t0.elapsed_ns());
        }
        wal.flush().expect("flush");
        drop(wal);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

impl SubMsRecipe for WalRecipe {
    fn name(&self) -> &str {
        "subms-ts-wal"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        Self::drive(h, "append_buffered", rounds, TsFsyncPolicy::Never);
        // The fsync interval must sit below the p99 tail: at 1-in-N appends the
        // fsync spike lands in the top 1/N of samples, so N must exceed 100 for
        // the per-append p99 to clear the fsync floor. 128 keeps the periodic
        // fsync in p999 rather than p99.
        Self::drive(
            h,
            "append_synced_n",
            rounds,
            TsFsyncPolicy::EveryNAppends(128),
        );

        h.add_meta("segment_max_records", "4096");
        h.add_meta("hardware_tier", "laptop");
        h.add_meta("subms.workload.feature", "write-ahead-log");
    }
}
