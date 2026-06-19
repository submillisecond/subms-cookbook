from subms_health import (
    ComponentHealth,
    EnvSection,
    FixedClock,
    HealthConfig,
    HealthRegistry,
    HealthStatus,
    MapEnv,
    ProbeKind,
    RedactionPolicy,
    RefreshPolicy,
    http_status_for,
    on_status_change,
)


def fixed_reg(config=None):
    return HealthRegistry(config, FixedClock(1000, "2026-06-18T00:00:00Z"))


def test_aggregate_worst_wins():
    S = HealthStatus
    assert S.aggregate([]) == S.UP
    assert S.aggregate([S.UP, S.UNKNOWN]) == S.UNKNOWN
    assert S.aggregate([S.UNKNOWN, S.WARN]) == S.WARN
    assert S.aggregate([S.WARN, S.DEGRADED]) == S.DEGRADED
    assert S.aggregate([S.DEGRADED, S.DOWN]) == S.DOWN


def test_tokens_and_http_mapping():
    assert HealthStatus.WARN.value == "WARN"
    assert http_status_for(HealthStatus.UP) == 200
    assert http_status_for(HealthStatus.WARN) == 200
    assert http_status_for(HealthStatus.DEGRADED) == 503
    assert http_status_for(HealthStatus.DOWN) == 503


def test_component_builders_and_json():
    d = ComponentHealth.down("boom")
    assert d.status == HealthStatus.DOWN
    assert d.to_json() == '{"status":"DOWN","details":{"error":"boom"}}'


def test_nested_effective_status():
    p = ComponentHealth.up().with_subcomponent("b", ComponentHealth.down("x"))
    assert p.status == HealthStatus.UP
    assert p.effective_status() == HealthStatus.DOWN


def test_registry_all_up():
    reg = fixed_reg()
    reg.register_fn("a", ComponentHealth.up)
    code, _ = reg.render()
    assert code == 200
    assert reg.status() == HealthStatus.UP


def test_critical_down_is_down():
    reg = fixed_reg()
    reg.register_fn("db", lambda: ComponentHealth.down("gone"), RefreshPolicy(critical=True))
    code, _ = reg.render()
    assert code == 503
    assert reg.status() == HealthStatus.DOWN


def test_non_critical_down_is_warn():
    reg = fixed_reg()
    reg.register_fn("cache", lambda: ComponentHealth.down("gone"), RefreshPolicy(critical=False))
    code, body = reg.render()
    assert code == 200
    assert reg.status() == HealthStatus.WARN
    assert '"status":"DOWN"' in body


def test_probe_kind_filtering():
    reg = fixed_reg()
    reg.register_fn("live", ComponentHealth.up, RefreshPolicy(probe_kinds=[ProbeKind.LIVENESS]))
    reg.register_fn(
        "ready", lambda: ComponentHealth.down("x"), RefreshPolicy(probe_kinds=[ProbeKind.READINESS])
    )
    lc, lj = reg.render_liveness()
    assert lc == 200 and "live" in lj and "ready" not in lj
    rc, _ = reg.render_readiness()
    assert rc == 503
    sc, _ = reg.render_startup()
    assert sc == 200


def test_degraded_fails_readiness_not_liveness():
    reg = fixed_reg()
    reg.register_fn(
        "engine",
        lambda: ComponentHealth.degraded("backpressure"),
        RefreshPolicy(critical=True, probe_kinds=[ProbeKind.LIVENESS, ProbeKind.READINESS]),
    )
    assert reg.render_readiness()[0] == 503
    assert reg.render_liveness()[0] == 200


def test_env_explicit_prefix_glob():
    env = MapEnv().with_("KICKSTART_A", "1").with_("APP_URL", "http://x").with_("OTHER", "n")
    assert len(EnvSection("d").keys(["KICKSTART_A"]).render(env).details) == 1
    assert len(EnvSection("d").prefix("KICKSTART_").render(env).details) == 1
    assert len(EnvSection("d").glob("*_URL").render(env).details) == 1


def test_redaction_policies():
    v = "supersecretvalue"
    assert RedactionPolicy.MASK.apply(v) == "***"
    assert RedactionPolicy.LAST4.apply(v) == "***alue"
    fp = RedactionPolicy.FINGERPRINT.apply(v)
    assert fp.startswith("fp_") and len(fp) == 9
    assert RedactionPolicy.HASH.apply(v).startswith("fnv1a:")
    assert RedactionPolicy.FINGERPRINT.apply("a") != RedactionPolicy.FINGERPRINT.apply("b")


def test_env_remap_strip_lowercase():
    env = MapEnv().with_("KICKSTART_TARGET", "edge").with_("KICKSTART_ENV", "prod")
    c = (
        EnvSection("d")
        .prefix("KICKSTART_")
        .strip_prefix_in_key(True)
        .lowercase_keys(True)
        .remap("KICKSTART_TARGET", "where")
        .render(env)
    )
    assert c.details["where"] == "edge"
    assert "env" in c.details


def test_cross_language_env_section_fixture():
    env = (
        MapEnv()
        .with_("KICKSTART_ENV", "prod")
        .with_("KICKSTART_VERSION", "1.2.3")
        .with_("KICKSTART_TOKEN", "supersecret")
        .with_("OTHER", "ignore")
    )
    section = (
        EnvSection("deploy")
        .prefix("KICKSTART_")
        .strip_prefix_in_key(True)
        .lowercase_keys(True)
        .redact_secrets()
    )
    assert section.render(env).to_json() == (
        '{"status":"UP","details":{"env":"prod","token":"***","version":"1.2.3"}}'
    )


def test_cross_language_report_fixture():
    reg = fixed_reg()
    reg.register_fn("db", lambda: ComponentHealth.up().with_detail("ping", "ok"), RefreshPolicy(critical=True))
    reg.register_fn("cache", lambda: ComponentHealth.down("conn refused"), RefreshPolicy(critical=False))
    code, body = reg.render()
    assert code == 200
    assert body == (
        '{"status":"WARN","refreshed_at":"2026-06-18T00:00:00Z","components":{'
        '"cache":{"status":"DOWN","age_ms":0,"stale":false,"details":{"error":"conn refused"}},'
        '"db":{"status":"UP","age_ms":0,"stale":false,"details":{"ping":"ok"}}}}'
    )


def test_json_escaping():
    c = ComponentHealth.up().with_detail("msg", 'a"b\\c\nd\te')
    assert 'a\\"b\\\\c\\nd\\te' in c.to_json()


def test_empty_registry():
    reg = fixed_reg()
    code, body = reg.render()
    assert code == 200
    assert body == '{"status":"UP","refreshed_at":"2026-06-18T00:00:00Z"}'


def test_staleness_flag():
    clock = FixedClock(1000, "2026-06-18T00:00:00Z")
    reg = HealthRegistry(HealthConfig(stale_factor=0.5), clock)
    reg.register_fn("x", ComponentHealth.up, RefreshPolicy(interval_ms=100))
    reg.refresh_now()
    clock.set(1080)
    reg.refresh_due()
    _, body = reg.render()
    assert '"age_ms":80' in body
    assert '"stale":true' in body


def test_refresh_picks_up_mutation():
    state = {"down": False}
    reg = fixed_reg()
    reg.register_fn(
        "flappy",
        lambda: ComponentHealth.down("x") if state["down"] else ComponentHealth.up(),
        RefreshPolicy(critical=True),
    )
    assert reg.status() == HealthStatus.UP
    state["down"] = True
    reg.refresh_now()
    assert reg.status() == HealthStatus.DOWN


def test_status_change_callback_sync_dispatch():
    seen = []
    reg = fixed_reg(HealthConfig.sync())
    reg.add_listener(on_status_change(lambda e: seen.append((e.attr("scope"), e.attr("from"), e.attr("to")))))
    state = {"down": False}
    reg.register_fn(
        "api",
        lambda: ComponentHealth.down("503") if state["down"] else ComponentHealth.up(),
        RefreshPolicy(critical=True),
    )
    reg.refresh_now()  # baseline
    state["down"] = True
    reg.refresh_now()  # UP -> DOWN
    overall = [c for c in seen if c[0] == "overall"]
    assert ("overall", "UP", "DOWN") in overall


def test_with_system_sections_smoke():
    reg = HealthRegistry.with_system_sections()
    code, body = reg.render()
    assert code in (200, 503)
    assert "server" in body
