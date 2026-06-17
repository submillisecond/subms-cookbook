package com.submillisecond.otel;

import io.opentelemetry.api.common.AttributeKey;

/**
 * Stable OpenTelemetry {@link AttributeKey} constants for every attribute the subms bridge emits.
 *
 * <p>Consumers build Grafana panels, Prometheus rules, or OTLP filters off these names; they are part of the public
 * API contract. The Rust sibling carries the byte-equivalent string constants.
 */
public final class SubMsOtelAttributeKeys {

    public static final String WORKLOAD          = "subms.workload";
    public static final String LANG              = "subms.lang";
    public static final String STAGE             = "subms.stage";
    public static final String STAGE_KIND        = "subms.stage.kind";
    public static final String RECIPE_SLUG       = "subms.recipe.slug";
    public static final String RECIPE_CATEGORY   = "subms.recipe.category";
    public static final String WORKLOAD_FEATURE  = "subms.workload.feature";
    public static final String WORKLOAD_ENTRIES  = "subms.workload.entries";
    public static final String WORKLOAD_SEED     = "subms.workload.seed";
    public static final String HOST              = "subms.host";
    public static final String HARDWARE_TIER     = "subms.hardware.tier";
    public static final String CRATE_VERSION     = "subms.crate.version";

    public static final AttributeKey<String> KEY_WORKLOAD         = AttributeKey.stringKey(WORKLOAD);
    public static final AttributeKey<String> KEY_LANG             = AttributeKey.stringKey(LANG);
    public static final AttributeKey<String> KEY_STAGE            = AttributeKey.stringKey(STAGE);
    public static final AttributeKey<String> KEY_STAGE_KIND       = AttributeKey.stringKey(STAGE_KIND);
    public static final AttributeKey<String> KEY_RECIPE_SLUG      = AttributeKey.stringKey(RECIPE_SLUG);
    public static final AttributeKey<String> KEY_RECIPE_CATEGORY  = AttributeKey.stringKey(RECIPE_CATEGORY);
    public static final AttributeKey<String> KEY_WORKLOAD_FEATURE = AttributeKey.stringKey(WORKLOAD_FEATURE);
    public static final AttributeKey<String> KEY_WORKLOAD_ENTRIES = AttributeKey.stringKey(WORKLOAD_ENTRIES);
    public static final AttributeKey<String> KEY_WORKLOAD_SEED    = AttributeKey.stringKey(WORKLOAD_SEED);
    public static final AttributeKey<String> KEY_HOST             = AttributeKey.stringKey(HOST);
    public static final AttributeKey<String> KEY_HARDWARE_TIER    = AttributeKey.stringKey(HARDWARE_TIER);
    public static final AttributeKey<String> KEY_CRATE_VERSION    = AttributeKey.stringKey(CRATE_VERSION);

    private SubMsOtelAttributeKeys() {}
}
