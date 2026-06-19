//! `SubMsRecipe` impl. Three stages: `register` (build + register indicators),
//! `report` (force-probe + aggregate + serialise the snapshot), `render_json`
//! (serve the cached snapshot - the per-request hot path).

use std::sync::Arc;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{ComponentHealth, EnvSection, HealthRegistry, MapEnv, RefreshPolicy};

pub struct HealthRecipe;

const INDICATORS: usize = 16;
const ENV_VARS_PER_SECTION: usize = 8;

fn build_registry() -> HealthRegistry {
    let mut reg = HealthRegistry::new();
    for i in 0..(INDICATORS - 2) {
        let name = format!("dep-{i}");
        let healthy = i % 5 != 0;
        reg.register_fn(
            &name,
            RefreshPolicy::new().with_interval_ms(1_000),
            move || {
                if healthy {
                    ComponentHealth::up()
                        .with_detail("ping", "ok")
                        .with_detail("rtt_us", 42i64)
                } else {
                    ComponentHealth::degraded("slow upstream").with_detail("rtt_us", 9_000i64)
                }
            },
        );
    }
    for s in 0..2 {
        let mut env = MapEnv::new();
        for v in 0..ENV_VARS_PER_SECTION {
            env = env.with(&format!("KICKSTART_VAR{s}_{v}"), &format!("value-{s}-{v}"));
        }
        let section = EnvSection::new(&format!("deploy-{s}"))
            .prefix("KICKSTART_")
            .strip_prefix_in_key(true)
            .lowercase_keys(true)
            .redact_secrets();
        reg.register(
            Arc::new(section.into_indicator(Arc::new(env))),
            RefreshPolicy::new()
                .with_interval_ms(60_000)
                .critical(false),
        );
    }
    reg
}

impl SubMsRecipe for HealthRecipe {
    fn name(&self) -> &str {
        "subms-health"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;

        let s_reg = h
            .stage("register", entries)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let reg = build_registry();
            s_reg.record(t0.elapsed_ns());
            std::hint::black_box(&reg);
        }

        let reg = build_registry();
        let s_report = h
            .stage("report", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            reg.refresh_now();
            s_report.record(t0.elapsed_ns());
        }

        reg.refresh_now();
        let s_render = h
            .stage("render_json", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let (code, json) = reg.render();
            s_render.record(t0.elapsed_ns());
            std::hint::black_box((code, json.len()));
        }

        let (_, json) = reg.render();
        h.add_meta("indicators", &INDICATORS.to_string());
        h.add_meta("env_vars_per_section", &ENV_VARS_PER_SECTION.to_string());
        h.add_meta("report_bytes", &json.len().to_string());
        h.add_meta("overall_status", reg.status().as_str());
        h.add_meta("subms.workload.feature", "cached-snapshot-render");
    }
}
