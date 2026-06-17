package com.submillisecond.recipes.tsplan;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class TsPlanTest {

    private static TsPlan samplePlan() {
        return new TsPlan()
                .then("subms-zone-map", "candidates", 500_000)
                .then("subms-gorilla-block", "range_scan", 37_100)
                .then("subms-ts", "range_min", 900)
                .then("subms-tdigest", "quantile", 300)
                .withOverhead(50_000);
    }

    // Pins the canonical certificate JSON (and its FNV-1a integrity) so the Java
    // port produces byte-identical output to the Rust crate.
    private static final String CERT_FIXTURE =
            "{\"hardware_tier\":\"ci-dedicated\",\"total_p99_ns\":588300,\"planner_overhead_ns\":50000,"
            + "\"valid_until\":0,\"stages\":[{\"recipe\":\"subms-zone-map\",\"stage\":\"candidates\",\"p99_ns\":500000},"
            + "{\"recipe\":\"subms-gorilla-block\",\"stage\":\"range_scan\",\"p99_ns\":37100},"
            + "{\"recipe\":\"subms-ts\",\"stage\":\"range_min\",\"p99_ns\":900},"
            + "{\"recipe\":\"subms-tdigest\",\"stage\":\"quantile\",\"p99_ns\":300}],"
            + "\"integrity\":13556477715296242473}";

    @Test
    void totalIsSumPlusOverhead() {
        TsPlan p = samplePlan();
        assertEquals(500_000 + 37_100 + 900 + 300 + 50_000, p.totalP99Ns());
    }

    @Test
    void emptyPlanIsOverheadOnly() {
        TsPlan p = new TsPlan().withOverhead(1_234);
        assertEquals(1_234, p.totalP99Ns());
        TsPlan bare = new TsPlan();
        assertEquals(0, bare.totalP99Ns());
    }

    @Test
    void certificateMeetsBudget() {
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        assertTrue(cert.meetsBudget(1_000_000)); // < 1 ms
        assertFalse(cert.meetsBudget(500_000)); // 588_300 > 500_000
        assertEquals(588_300, cert.totalP99Ns());
    }

    @Test
    void certificateCarriesStages() {
        TsLatencyCertificate cert = samplePlan().certify("laptop", 42);
        assertEquals(4, cert.stages().size());
        assertEquals("laptop", cert.hardwareTier());
        assertEquals(42, cert.validUntil());
        assertEquals("subms-zone-map", cert.stages().get(0).recipe());
    }

    // Cross-language byte-equivalence: same canonical JSON AND same FNV integrity
    // as the Rust sibling. The unsigned integrity renders as 13556477715296242473.
    @Test
    void jsonMatchesFixture() {
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        assertEquals(CERT_FIXTURE, cert.toJson());
        assertEquals(Long.parseUnsignedLong("13556477715296242473"), cert.integrity());
    }

    @Test
    void integrityVerifies() {
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        assertTrue(cert.verify());
    }

    @Test
    void tamperBreaksIntegrity() {
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        assertTrue(cert.verify());
        // someone edited the headline number but kept the old integrity hash.
        TsLatencyCertificate tampered = new TsLatencyCertificate(
                cert.hardwareTier(),
                cert.totalP99Ns() + 1,
                cert.plannerOverheadNs(),
                cert.validUntil(),
                cert.stages(),
                cert.integrity());
        assertFalse(tampered.verify());
    }

    @Test
    void tamperOnAStageBreaksIntegrity() {
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        var stages = new java.util.ArrayList<>(cert.stages());
        TsPlanStage s = stages.get(1);
        stages.set(1, new TsPlanStage(s.recipe(), s.stage(), 1)); // understate a stage
        TsLatencyCertificate tampered = new TsLatencyCertificate(
                cert.hardwareTier(),
                cert.totalP99Ns(),
                cert.plannerOverheadNs(),
                cert.validUntil(),
                stages,
                cert.integrity());
        assertFalse(tampered.verify());
    }

    @Test
    void tierChangeChangesHash() {
        TsLatencyCertificate a = samplePlan().certify("laptop", 0);
        TsLatencyCertificate b = samplePlan().certify("ci-dedicated", 0);
        assertNotEquals(a.integrity(), b.integrity());
    }

    @Test
    void saturatingTotalDoesNotWrap() {
        TsPlan p = new TsPlan()
                .then("a", "x", -1L) // u64::MAX
                .then("b", "y", -1L);
        assertEquals(-1L, p.totalP99Ns()); // unsigned max, not a wrap
    }

    @Test
    void jsonRoundTripsThroughFields() {
        // rebuild a certificate from the parsed-out fields and confirm the hash
        // is stable (the body is a pure function of the fields).
        TsLatencyCertificate cert = samplePlan().certify("ci-dedicated", 0);
        TsLatencyCertificate rebuilt = new TsLatencyCertificate(
                cert.hardwareTier(),
                cert.totalP99Ns(),
                cert.plannerOverheadNs(),
                cert.validUntil(),
                cert.stages(),
                cert.integrity());
        assertTrue(rebuilt.verify());
        assertEquals(cert.toJson(), rebuilt.toJson());
    }

    @Test
    void stageStringsAreEscaped() {
        TsLatencyCertificate cert = new TsPlan()
                .then("re\"cipe", "st\\age", 10)
                .certify("tier\"x", 0);
        String json = cert.toJson();
        assertTrue(json.contains("re\\\"cipe"));
        assertTrue(json.contains("st\\\\age"));
        assertTrue(json.contains("tier\\\"x"));
        assertTrue(cert.verify());
    }

    @Test
    void newlineAndTabAreEscaped() {
        TsLatencyCertificate cert = new TsPlan()
                .then("a\nb", "c\td", 1)
                .certify("e\rf", 0);
        String json = cert.toJson();
        assertTrue(json.contains("a\\nb"));
        assertTrue(json.contains("c\\td"));
        assertTrue(json.contains("e\\rf"));
        assertTrue(cert.verify());
    }

    @Test
    void planStageIsPublic() {
        TsPlanStage s = new TsPlanStage("r", "s", 1);
        TsPlan p = new TsPlan().then(s.recipe(), s.stage(), s.p99Ns());
        assertEquals(s, p.stages().get(0));
    }

    @Test
    void plannerOverheadAccessor() {
        TsPlan p = new TsPlan().withOverhead(7_777);
        assertEquals(7_777, p.plannerOverheadNs());
        assertEquals(7_777, p.certify("t", 0).plannerOverheadNs());
    }
}
