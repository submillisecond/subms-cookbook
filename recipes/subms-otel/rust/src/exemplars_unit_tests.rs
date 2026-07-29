use super::*;

fn ctx<'a>(stage: &'a str) -> ObservationCtx<'a> {
    ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage,
        stage_kind: SubMsStageKind::HotPath,
    }
}

#[test]
fn bucket_index_partitions_by_kind() {
    // HotPath bounds start at 5e-8; 10 ns is below first boundary.
    assert_eq!(bucket_index_for(SubMsStageKind::HotPath, 10), 0);
    // 100 ns -> bound 1e-7 (idx 1).
    assert_eq!(bucket_index_for(SubMsStageKind::HotPath, 100), 1);
    // 2 ms > 1e-3 -> overflow.
    let hot_overflow = bucket_index_for(SubMsStageKind::HotPath, 2_000_000);
    assert_eq!(
        hot_overflow,
        histogram_boundaries(SubMsStageKind::HotPath).len()
    );
}

#[test]
fn reservoir_keeps_slowest_k() {
    let r = ExemplarReservoir::with_capacity(3);
    for ns in [1u64, 2, 3, 4, 5] {
        r.offer(&ctx("put"), ns);
    }
    let snap = r.snapshot();
    let kept: Vec<u64> = snap.iter().map(|e| e.ns).collect();
    assert_eq!(kept, vec![3, 4, 5], "slowest 3 kept");
}
