use subms_ts::{TsCollection, TsSeriesMetadata, TsTags};
use subms_ts_promql::{TsPromQl, TsPromQlError};

// Build a small collection of named, tagged counter series for the suite.
// http_requests_total: three series under job=api/api/web, climbing counters.
fn fixture() -> TsCollection<f64> {
    let mut coll = TsCollection::<f64>::new();

    type Spec = (u64, &'static str, &'static str, &'static [(i64, f64)]);
    let specs: &[Spec] = &[
        (
            1,
            "api",
            "i-1",
            &[(0, 0.0), (60_000_000_000, 60.0), (120_000_000_000, 120.0)],
        ),
        (
            2,
            "api",
            "i-2",
            &[(0, 0.0), (60_000_000_000, 30.0), (120_000_000_000, 60.0)],
        ),
        (
            3,
            "web",
            "i-3",
            &[(0, 0.0), (60_000_000_000, 10.0), (120_000_000_000, 20.0)],
        ),
    ];

    for (id, job, inst, pts) in specs {
        // Many series share one metric name, distinguished by labels - so the
        // metric goes in the __name__ tag and the registry name stays empty.
        let meta = TsSeriesMetadata::new(*id, "")
            .with_tag("__name__", "http_requests_total")
            .with_tag("job", *job)
            .with_tag("instance", *inst);
        coll.register(meta).unwrap();
        for (ts, v) in *pts {
            coll.push(*id, *ts, *v).unwrap();
        }
    }
    coll
}

fn tags(pairs: &[(&str, &str)]) -> TsTags {
    let mut t = TsTags::new();
    for (k, v) in pairs {
        t.insert((*k).to_string(), (*v).to_string());
    }
    t
}

#[test]
fn instant_selector_eq_matcher_resolves_right_series() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("http_requests_total{job=\"web\"}", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res.samples()[0].value, 20.0);
    assert_eq!(res.samples()[0].labels.get("instance").unwrap(), "i-3");
}

#[test]
fn bare_selector_resolves_all_series_of_metric() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("http_requests_total", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 3);
}

#[test]
fn ne_matcher_excludes() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("http_requests_total{job!=\"web\"}", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 2);
    assert!(
        res.samples()
            .iter()
            .all(|s| s.labels.get("job").unwrap() == "api")
    );
}

#[test]
fn re_matcher_with_dotstar() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // "a.*" matches "api" but not "web".
    let res = q
        .eval_instant("http_requests_total{job=~\"a.*\"}", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 2);

    // "i-.*2" anchors front + back: matches instance i-2 only.
    let res2 = q
        .eval_instant("http_requests_total{instance=~\"i-.*2\"}", 120_000_000_000)
        .unwrap();
    assert_eq!(res2.len(), 1);
    assert_eq!(res2.samples()[0].labels.get("instance").unwrap(), "i-2");
}

#[test]
fn nre_matcher_negates() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("http_requests_total{job!~\"a.*\"}", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res.samples()[0].labels.get("job").unwrap(), "web");
}

#[test]
fn sum_by_job_groups_and_sums() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("sum by (job) (http_requests_total)", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 2);
    // api = 120 + 60 = 180; web = 20.
    assert_eq!(res.value_for(&tags(&[("job", "api")])).unwrap(), 180.0);
    assert_eq!(res.value_for(&tags(&[("job", "web")])).unwrap(), 20.0);
}

#[test]
fn avg_min_max_count_aggregations() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let at = 120_000_000_000;

    let avg = q
        .eval_instant("avg by (job) (http_requests_total)", at)
        .unwrap();
    assert_eq!(avg.value_for(&tags(&[("job", "api")])).unwrap(), 90.0); // (120+60)/2

    let min = q
        .eval_instant("min by (job) (http_requests_total)", at)
        .unwrap();
    assert_eq!(min.value_for(&tags(&[("job", "api")])).unwrap(), 60.0);

    let max = q
        .eval_instant("max by (job) (http_requests_total)", at)
        .unwrap();
    assert_eq!(max.value_for(&tags(&[("job", "api")])).unwrap(), 120.0);

    let count = q
        .eval_instant("count by (job) (http_requests_total)", at)
        .unwrap();
    assert_eq!(count.value_for(&tags(&[("job", "api")])).unwrap(), 2.0);
    assert_eq!(count.value_for(&tags(&[("job", "web")])).unwrap(), 1.0);
}

#[test]
fn aggregation_without() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // drop instance -> group by job (the only other label).
    let res = q
        .eval_instant(
            "sum without (instance) (http_requests_total)",
            120_000_000_000,
        )
        .unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(res.value_for(&tags(&[("job", "api")])).unwrap(), 180.0);
}

#[test]
fn aggregation_no_grouping_collapses_all() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("sum(http_requests_total)", 120_000_000_000)
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res.samples()[0].value, 200.0); // 120 + 60 + 20
    assert!(res.samples()[0].labels.is_empty());
}

#[test]
fn rate_over_counter_gives_per_second_slope() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // api i-1 climbs 0->120 over 120s -> 1.0/s. range covers the whole window.
    let res = q
        .eval_instant(
            "rate(http_requests_total{instance=\"i-1\"}[5m])",
            120_000_000_000,
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert!((res.samples()[0].value - 1.0).abs() < 1e-9);

    // i-2 climbs 0->60 over 120s -> 0.5/s.
    let res2 = q
        .eval_instant(
            "rate(http_requests_total{instance=\"i-2\"}[5m])",
            120_000_000_000,
        )
        .unwrap();
    assert!((res2.samples()[0].value - 0.5).abs() < 1e-9);
}

#[test]
fn irate_uses_last_two_samples() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // i-1 last step 60->120 over 60s -> 1.0/s.
    let res = q
        .eval_instant(
            "irate(http_requests_total{instance=\"i-1\"}[5m])",
            120_000_000_000,
        )
        .unwrap();
    assert!((res.samples()[0].value - 1.0).abs() < 1e-9);
}

#[test]
fn increase_is_total_delta() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant(
            "increase(http_requests_total{instance=\"i-1\"}[5m])",
            120_000_000_000,
        )
        .unwrap();
    assert!((res.samples()[0].value - 120.0).abs() < 1e-9);
}

#[test]
fn binary_op_division_with_label_matching() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // sum by(job) / count by(job) = avg by(job).
    let res = q
        .eval_instant(
            "sum by (job) (http_requests_total) / count by (job) (http_requests_total)",
            120_000_000_000,
        )
        .unwrap();
    assert_eq!(res.value_for(&tags(&[("job", "api")])).unwrap(), 90.0);
    assert_eq!(res.value_for(&tags(&[("job", "web")])).unwrap(), 20.0);
}

#[test]
fn scalar_binary_broadcast() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("sum(http_requests_total) / 2", 120_000_000_000)
        .unwrap();
    assert_eq!(res.samples()[0].value, 100.0);
}

#[test]
fn offset_shifts_eval_point() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // at 120s with offset 60s -> read as-of 60s -> i-1 value is 60.
    let res = q
        .eval_instant(
            "http_requests_total{instance=\"i-1\"} offset 1m",
            120_000_000_000,
        )
        .unwrap();
    assert_eq!(res.samples()[0].value, 60.0);
}

#[test]
fn unknown_metric_is_empty() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_instant("nonexistent_metric", 120_000_000_000)
        .unwrap();
    assert!(res.is_empty());
}

#[test]
fn parse_error_on_malformed_query() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let err = q
        .eval_instant("sum by (job (http_requests_total)", 0)
        .unwrap_err();
    assert!(matches!(err, TsPromQlError::Parse(_)));

    let err2 = q.eval_instant("http_requests_total{job=}", 0).unwrap_err();
    assert!(matches!(err2, TsPromQlError::Parse(_)));

    let err3 = q.eval_instant("", 0).unwrap_err();
    assert!(matches!(err3, TsPromQlError::Parse(_)));
}

#[test]
fn range_eval_steps_over_window() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    let res = q
        .eval_range(
            "sum(http_requests_total)",
            0,
            120_000_000_000,
            60_000_000_000,
        )
        .unwrap();
    assert_eq!(res.len(), 3);
    // at t=0 all counters are 0; at t=120s sum is 200.
    assert_eq!(res.steps()[0].result.samples()[0].value, 0.0);
    assert_eq!(res.steps()[2].result.samples()[0].value, 200.0);
}

#[test]
fn range_eval_rejects_bad_args() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    assert!(matches!(
        q.eval_range("http_requests_total", 0, 10, 0),
        Err(TsPromQlError::Eval(_))
    ));
    assert!(matches!(
        q.eval_range("http_requests_total", 10, 0, 1),
        Err(TsPromQlError::Eval(_))
    ));
}

#[test]
fn duration_units_parse() {
    let coll = fixture();
    let q = TsPromQl::new(&coll);
    // 1h covers the whole window; rate over it is the same 1.0/s slope.
    let res = q
        .eval_instant(
            "rate(http_requests_total{instance=\"i-1\"}[1h])",
            120_000_000_000,
        )
        .unwrap();
    assert!((res.samples()[0].value - 1.0).abs() < 1e-9);
}

#[test]
fn counter_reset_handled_in_rate() {
    // a counter that resets mid-window: 0,100,(reset)10,30.
    let mut coll = TsCollection::<f64>::new();
    let meta = TsSeriesMetadata::new(1, "")
        .with_tag("__name__", "c")
        .with_tag("job", "x");
    coll.register(meta).unwrap();
    for (ts, v) in [
        (0i64, 0.0),
        (10_000_000_000, 100.0),
        (20_000_000_000, 10.0),
        (30_000_000_000, 30.0),
    ] {
        coll.push(1, ts, v).unwrap();
    }
    let q = TsPromQl::new(&coll);
    // deltas: +100, reset->+10, +20 = 130 over 30s ~ 4.333/s.
    let res = q.eval_instant("rate(c[5m])", 30_000_000_000).unwrap();
    assert!((res.samples()[0].value - (130.0 / 30.0)).abs() < 1e-9);
}
