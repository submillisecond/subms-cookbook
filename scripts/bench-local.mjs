#!/usr/bin/env node
// Standardised cookbook bench runner + verifier (LOCAL).
//
// Walks `recipes/`, runs every recipe's rust + java perf bench, captures the
// JSON to `<recipe>/perf/local/<lang>.json`, then verifies those captures hold
// the sub-millisecond p99 claim. LOCAL captures NEVER touch the official
// `<recipe>/perf/<lang>.json` - those are the system of record, written only by
// bench-on-fleet (subms-infra) on an isolated EC2 box, so a laptop run can't
// clobber the numbers the site renders. `--check` (no capture) verifies the
// OFFICIAL perf/ instead: it is the CI conformance gate.
//
// Usage:
//   node scripts/bench-local.mjs                  rust + java, every recipe
//   node scripts/bench-local.mjs --rust           rust only
//   node scripts/bench-local.mjs --java           java only
//   node scripts/bench-local.mjs --check          verify OFFICIAL perf/ (no run) - the conformance gate
//   node scripts/bench-local.mjs --only=foo,bar   filter to specific slugs
//   node scripts/bench-local.mjs --report-only    don't run, only print table
//   node scripts/bench-local.mjs --parallel=N     up to N recipes at once (default 1)
//   node scripts/bench-local.mjs --clean          delete existing LOCAL (perf/local) JSONs first
//   node scripts/bench-local.mjs --threshold=NS   override the 1ms (1_000_000ns) bar
//   node scripts/bench-local.mjs --md=PATH        write a markdown report
//   node scripts/bench-local.mjs --json=PATH      write a machine-readable summary
//   node scripts/bench-local.mjs --skip-warmup=N  drop the first N downsampled
//                                               samples (default 50 = 10%) so
//                                               JIT-cold / cache-cold passes
//                                               don't pollute the percentile.
//   node scripts/bench-local.mjs --top=N          rows in best/worst tables (default 8)
//
// Exit codes:
//   0  all stages under threshold
//   1  one or more stages breached
//   2  one or more recipe-lang files missing
//   3  one or more bench commands failed
//
// Baseline detection: a run is treated as a comparison baseline (and so
// excluded from threshold enforcement) when ANY of:
//   - meta.baseline === "true"
//   - inputs.bloom_mode === "off"
//   - any input value ends with "_off" or equals "off"
//   - workload string contains "baseline" or "without"

import { readdir, readFile, writeFile, mkdir, rm, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { spawn } from "node:child_process";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const COOKBOOK_ROOT = join(__dirname, "..");
const RECIPES_ROOT = join(COOKBOOK_ROOT, "recipes");

// ---------------- arg parsing ----------------
const args = process.argv.slice(2);
const flag = (n) => args.includes(`--${n}`);
const value = (n) => {
  const a = args.find((x) => x.startsWith(`--${n}=`));
  return a ? a.slice(n.length + 3) : null;
};
const langFiltered = flag("rust") || flag("java");
const runRust = !flag("check") && !flag("report-only") && (!langFiltered || flag("rust"));
const runJava = !flag("check") && !flag("report-only") && (!langFiltered || flag("java"));
const onlyFilter = value("only")?.split(",").map((s) => s.trim()).filter(Boolean) ?? null;
const parallel = Math.max(1, Number(value("parallel") ?? "1") || 1);
const threshold = Math.max(1, Number(value("threshold") ?? "1000000") || 1_000_000);
// Run-level report artifacts (cross-recipe, ephemeral, gitignored) live under the
// cookbook's local scratch dir, mirroring the per-recipe recipes/<slug>/.subms/local/
// convention - not the repo root. Override with --md=/--json=.
const mdPath = value("md") ?? join(COOKBOOK_ROOT, ".subms", "local", "BENCH_REPORT.md");
const jsonPath = value("json") ?? join(COOKBOOK_ROOT, ".subms", "local", "BENCH_REPORT.json");
const cleanFirst = flag("clean");
const skipWarmup = Math.max(0, Number(value("skip-warmup") ?? "50") || 0);
const topN = Math.max(1, Number(value("top") ?? "8") || 8);
// A local bench capture ALWAYS writes to the GIT-IGNORED
// `<recipe>/.subms/local/perf/<lang>.json`, never the official git-tracked
// `<recipe>/.subms/perf/<lang>.json`. The official files (the numbers the
// site renders and CI checks) are the system of record and come ONLY from
// bench-on-fleet (subms-infra) on an isolated EC2 box - this local runner has no
// way to write them, by design, so a laptop run can never clobber the fleet data.
const LOCAL_SEGMENTS = [".subms", "local", "perf"];
const OFFICIAL_SEGMENTS = [".subms", "perf"];
const perfDest = (slug, lang) => join(RECIPES_ROOT, slug, ...LOCAL_SEGMENTS, `${lang}.json`);
// A capture run writes AND verifies the local files it produced. A standalone
// --check (the conformance gate, no capture) verifies the OFFICIAL captures - the
// committed, fleet-captured numbers. So the verify path follows the mode.
const capturing = runRust || runJava;
const verifyDest = (slug, lang) =>
  join(RECIPES_ROOT, slug, ...(capturing ? LOCAL_SEGMENTS : OFFICIAL_SEGMENTS), `${lang}.json`);
// Max points each stage stores in `samples_ns` (subms `sample_cap`; default
// 500 in-crate). Raise it (e.g. --samples=50000 / SUBMS_SAMPLES=50000) so the
// stored timeline is dense enough for an exact percentile recompute + a full
// raw dataset. Fed to perf_main's stdin; unset → the crate default (500).
const sampleCap = Math.max(0, Number(value("samples") ?? process.env.SUBMS_SAMPLES ?? "0") || 0) || null;
const benchStdin = sampleCap ? `sample_cap=${sampleCap}\n` : null;

// ---------------- ansi helpers ----------------
const COLOR = process.stdout.isTTY && !process.env.NO_COLOR;
const c = {
  reset: COLOR ? "\x1b[0m" : "",
  bold: COLOR ? "\x1b[1m" : "",
  dim: COLOR ? "\x1b[2m" : "",
  red: COLOR ? "\x1b[31m" : "",
  green: COLOR ? "\x1b[32m" : "",
  yellow: COLOR ? "\x1b[33m" : "",
  cyan: COLOR ? "\x1b[36m" : "",
  gray: COLOR ? "\x1b[90m" : "",
};

// ---------------- recipe discovery ----------------
const allSlugs = (await readdir(RECIPES_ROOT, { withFileTypes: true }))
  .filter((e) => e.isDirectory() && e.name.startsWith("subms-"))
  .map((e) => e.name)
  .sort();
const slugs = onlyFilter ? allSlugs.filter((s) => onlyFilter.includes(s)) : allSlugs;

console.log();
console.log(`${c.bold}cookbook bench-local${c.reset}`);
console.log(`${c.dim}${slugs.length} recipe(s)${onlyFilter ? " (filtered)" : ""}  ·  rust=${runRust} java=${runJava}  ·  threshold=${(threshold/1000).toFixed(0)}us  ·  parallel=${parallel}${c.reset}`);
console.log();

// ---------------- runner helpers ----------------
function run(cmd, cmdArgs, opts = {}) {
  const { input, ...spawnOpts } = opts;
  return new Promise((resolve) => {
    const p = spawn(cmd, cmdArgs, {
      ...spawnOpts,
      stdio: [input != null ? "pipe" : "ignore", "pipe", "pipe"],
      shell: process.platform === "win32",
    });
    let out = "";
    let err = "";
    p.stdout.on("data", (b) => (out += b.toString()));
    p.stderr.on("data", (b) => (err += b.toString()));
    p.on("close", (code) => resolve({ code, out, err }));
    p.on("error", (e) => resolve({ code: -1, out: "", err: String(e) }));
    if (input != null) {
      p.stdin.on("error", () => {}); // ignore EPIPE if the child exits early
      p.stdin.write(input);
      p.stdin.end();
    }
  });
}

// The balanced [...] or {...} blob starting at `start` (bracket-aware, string-aware),
// or null if it never closes.
function balancedBlob(text, start) {
  const open = text[start];
  const close = open === "[" ? "]" : "}";
  let depth = 0, inStr = false, esc = false;
  for (let k = start; k < text.length; k++) {
    const ch = text[k];
    if (esc) { esc = false; continue; }
    if (inStr) { if (ch === "\\") esc = true; else if (ch === '"') inStr = false; continue; }
    if (ch === '"') { inStr = true; continue; }
    if (ch === open) depth++;
    else if (ch === close) { depth--; if (depth === 0) return text.slice(start, k + 1); }
  }
  return null;
}

// The harness summary object, robust to log noise before it. maven/cargo can print
// bracketed lines ([INFO]/[WARNING], plugin download progress) to stdout ahead of the
// JSON; a first-bracket-wins scan would grab "[INFO]" and fail. Instead try each `{`
// as a start, balance + parse, and return the first that IS a SubMs summary (has a
// `stages` object). Returns the parsed object + its raw text, or null.
function extractHarnessJson(text) {
  for (let s = text.indexOf("{"); s >= 0; s = text.indexOf("{", s + 1)) {
    const blob = balancedBlob(text, s);
    if (!blob) continue;
    let obj;
    try { obj = JSON.parse(blob); } catch { continue; }
    if (obj && typeof obj === "object" && obj.stages && typeof obj.stages === "object") {
      return { raw: blob, obj };
    }
  }
  return null;
}

async function findPerfMainClass(javaDir) {
  const srcRoot = join(javaDir, "src", "main", "java");
  if (!existsSync(srcRoot)) return null;
  async function walk(d) {
    let entries;
    try { entries = await readdir(d, { withFileTypes: true }); } catch { return null; }
    for (const e of entries) {
      const full = join(d, e.name);
      if (e.isDirectory()) { const hit = await walk(full); if (hit) return hit; }
      else if (e.name === "PerfMain.java") return full;
    }
    return null;
  }
  const file = await walk(srcRoot);
  if (!file) return null;
  return file.slice(srcRoot.length + 1, -".java".length).split(/[\\/]/).join(".");
}

// Turn a finished bench process into a result. A COMPLETE harness JSON in stdout is
// the success signal - not the exit code. If the process printed a full result yet
// exited non-zero, that is almost always a teardown/shutdown error AFTER the numbers
// were produced (an OS signal, a close() hiccup); keep the capture but log the exit so
// a genuine failure is not silently accepted. Only a MISSING/incomplete result fails.
async function finishBench(slug, lang, code, out, err, started) {
  const elapsed = Date.now() - started;
  const found = extractHarnessJson(out);
  if (!found) {
    const reason = code !== 0 ? (err.slice(-800) || `exit ${code}`) : "no harness JSON in stdout";
    return { status: "failed", reason, elapsed };
  }
  if (code !== 0) {
    console.log(`${c.yellow}  ${slug} ${lang}: exited ${code} after printing a complete result - keeping it (likely a teardown error). stderr tail:${c.reset}\n${err.slice(-400)}`);
  }
  const dest = perfDest(slug, lang);
  await mkdir(dirname(dest), { recursive: true });
  await writeFile(dest, found.raw + "\n");
  return { status: "ok", elapsed, path: dest };
}

async function benchRust(slug) {
  const rustDir = join(RECIPES_ROOT, slug, "rust");
  const example = join(rustDir, "examples", "perf_main.rs");
  if (!existsSync(example)) return { status: "skipped", reason: "no rust/examples/perf_main.rs" };
  const started = Date.now();
  const { code, out, err } = await run(
    "cargo",
    ["run", "--release", "--quiet", "--example", "perf_main", "--features", "harness"],
    { cwd: rustDir, input: benchStdin },
  );
  return finishBench(slug, "rust", code, out, err, started);
}

async function benchJava(slug) {
  const javaDir = join(RECIPES_ROOT, slug, "java");
  if (!existsSync(join(javaDir, "pom.xml"))) return { status: "skipped", reason: "no java/pom.xml" };
  const className = await findPerfMainClass(javaDir);
  if (!className) return { status: "skipped", reason: "no PerfMain.java" };
  const started = Date.now();
  // exec:java runs the bench IN maven's own JVM, so its heap is maven's heap. Left
  // unset that is the JVM's RAM-fraction default (~1/4 of RAM = ~256 MB on the 1 GB
  // fleet box), which is both non-deterministic across boxes and tight. Pin it via
  // MAVEN_OPTS so the number is reproducible; SUBMS_JAVA_XMX overrides, and a
  // caller-set MAVEN_OPTS wins outright.
  const mavenOpts = process.env.MAVEN_OPTS || `-Xmx${process.env.SUBMS_JAVA_XMX || "512m"}`;
  const { code, out, err } = await run(
    "mvn",
    ["-q", "compile", "exec:java", `-Dexec.mainClass=${className}`],
    { cwd: javaDir, input: benchStdin, env: { ...process.env, MAVEN_OPTS: mavenOpts } },
  );
  return finishBench(slug, "java", code, out, err, started);
}

// ---------------- baseline detection ----------------
function isBaselineRun(run) {
  if (!run || typeof run !== "object") return false;
  const meta = run.meta ?? {};
  const inputs = run.inputs ?? {};
  if (String(meta.baseline ?? "").toLowerCase() === "true") return true;
  if (String(inputs.bloom_mode ?? "").toLowerCase() === "off") return true;
  for (const v of Object.values(inputs)) {
    const s = String(v ?? "").toLowerCase();
    if (s === "off" || s.endsWith("_off") || s === "baseline") return true;
  }
  const wl = String(run.workload ?? "").toLowerCase();
  if (wl.includes("baseline") || wl.includes("without")) return true;
  return false;
}

// ---------------- clean ----------------
if (cleanFirst) {
  console.log(`${c.yellow}--clean: removing existing LOCAL perf JSONs (perf/local)${c.reset}`);
  for (const slug of slugs) {
    for (const lang of ["rust", "java"]) {
      const p = perfDest(slug, lang);
      if (existsSync(p)) await rm(p);
    }
  }
  console.log();
}

// ---------------- run pass (with bounded parallelism) ----------------
async function runRecipe(slug) {
  const out = { slug, rust: null, java: null };
  if (runRust) {
    const r = await benchRust(slug);
    out.rust = r;
  }
  if (runJava) {
    const r = await benchJava(slug);
    out.java = r;
  }
  // One compact line per recipe (printed when this slug completes).
  const fmt = (lang, r) => {
    if (!r) return `${c.gray}${lang}:skip${c.reset}`;
    if (r.status === "ok") return `${c.green}${lang}:✓${c.reset}${c.dim}/${(r.elapsed / 1000).toFixed(1)}s${c.reset}`;
    if (r.status === "skipped") return `${c.dim}${lang}:- ${r.reason}${c.reset}`;
    return `${c.red}${lang}:✗ ${r.reason.split("\n")[0].slice(0, 60)}${c.reset}`;
  };
  console.log(`  ${slug.padEnd(32)}  ${fmt("rust", out.rust)}  ${fmt("java", out.java)}`);
  return out;
}

const runResults = [];
if (runRust || runJava) {
  console.log(`${c.bold}Running benches${c.reset}`);
  // Bounded parallelism: process up to `parallel` recipes concurrently.
  const queue = slugs.slice();
  const workers = Array.from({ length: parallel }, async () => {
    while (queue.length) {
      const slug = queue.shift();
      if (!slug) break;
      const res = await runRecipe(slug);
      runResults.push(res);
    }
  });
  await Promise.all(workers);
  console.log();
}

// ---------------- verify pass ----------------
console.log(`${c.bold}Verifying${c.reset} ${c.dim}threshold p99 < ${(threshold / 1000).toFixed(0)}us${c.reset}`);
console.log();

const recipeReports = [];
let totalBreaches = 0;
let totalOk = 0;
let totalMissing = 0;

for (const slug of slugs) {
  const report = { slug, rust: null, java: null };
  for (const lang of ["rust", "java"]) {
    const path = verifyDest(slug, lang);
    const slot = await analysePerf(path, slug, lang);
    report[lang] = slot;
    if (slot.status === "missing") totalMissing++;
    else if (slot.status === "ok") totalOk++;
    else if (slot.status === "breach") totalBreaches++;
  }
  recipeReports.push(report);
}

// ---------------- table output ----------------
printTable(recipeReports);

// ---------------- summary ----------------
console.log();
const fmtCount = (n, label, color) => `${color}${n}${c.reset} ${label}`;
console.log(
  `${c.bold}Summary${c.reset}  ` +
    `${fmtCount(totalOk, "ok", c.green)}  ` +
    `${fmtCount(totalBreaches, "breach", c.red)}  ` +
    `${fmtCount(totalMissing, "missing", c.yellow)}`,
);
console.log();

// ---------------- writeReports ----------------
await mkdir(dirname(mdPath), { recursive: true });
await writeMarkdownReport(mdPath, recipeReports);
await writeJsonReport(jsonPath, recipeReports);
console.log(`${c.dim}report written: ${relative(COOKBOOK_ROOT, mdPath)}  +  ${relative(COOKBOOK_ROOT, jsonPath)}${c.reset}`);

// ---------------- exit ----------------
if (totalBreaches > 0) process.exit(1);
if (totalMissing > 0) process.exit(2);
const anyFail = runResults.some((r) => (r.rust?.status === "failed") || (r.java?.status === "failed"));
if (anyFail) process.exit(3);
process.exit(0);

// ===========================================================================
// helpers
// ===========================================================================

async function analysePerf(path, slug, lang) {
  let runs;
  try {
    const raw = await readFile(path, "utf8");
    runs = JSON.parse(raw);
  } catch (e) {
    if (e.code === "ENOENT") return { status: "missing", stages: [], worstP99: null };
    return { status: "parse-error", error: e.message, stages: [], worstP99: null };
  }
  if (!Array.isArray(runs)) runs = [runs];
  const stages = [];
  let worstP99 = 0;
  let breach = false;
  for (const run of runs) {
    const baseline = isBaselineRun(run);
    for (const [stage, v] of Object.entries(run.stages ?? {})) {
      // Recompute from the chronological samples_ns array, dropping the
      // first `skipWarmup` samples. samples_ns is downsampled to 500
      // evenly-spaced points (see the subms JSON contract), so 50 is
      // the first ~10% of the run - covers JIT cold-start in Java and
      // cache-cold passes in Rust. Original p50/p99 fields are kept
      // alongside as `raw_*` for comparison.
      const samples = Array.isArray(v.samples_ns) ? v.samples_ns : [];
      const recalc = samples.length > skipWarmup
        ? computeStats(samples.slice(skipWarmup))
        : null;
      const p50 = recalc?.p50 ?? v.p50_ns ?? 0;
      const p99 = recalc?.p99 ?? v.p99_ns ?? 0;
      const p999 = recalc?.p999 ?? v.p999_ns ?? 0;
      const max = recalc?.max ?? v.max_ns ?? 0;
      const mean = recalc?.mean ?? v.mean_ns ?? 0;
      const stddev = recalc?.stddev ?? 0;
      const enforced = !baseline;
      if (enforced && p99 > worstP99) worstP99 = p99;
      const stageBreach = enforced && p99 > threshold;
      if (stageBreach) breach = true;
      stages.push({
        slug,
        lang,
        workload: run.workload,
        stage,
        p50,
        p99,
        p999,
        max,
        mean,
        stddev,
        rawP99: v.p99_ns ?? 0,
        baseline,
        breach: stageBreach,
        sampleCount: samples.length,
        warmupSkipped: Math.min(skipWarmup, samples.length),
      });
    }
  }
  return {
    status: breach ? "breach" : "ok",
    stages,
    worstP99,
  };
}

// Compute p50/p99/p999/max/mean/stddev from a sample array.
function computeStats(samples) {
  if (samples.length === 0) return null;
  const sorted = [...samples].sort((a, b) => a - b);
  const n = sorted.length;
  const at = (q) => sorted[Math.min(n - 1, Math.max(0, Math.floor(q * n)))];
  const mean = samples.reduce((a, b) => a + b, 0) / n;
  let variance = 0;
  for (const s of samples) variance += (s - mean) * (s - mean);
  variance /= Math.max(1, n - 1);
  const stddev = Math.sqrt(variance);
  return {
    p50: at(0.50),
    p90: at(0.90),
    p99: at(0.99),
    p999: at(0.999),
    max: sorted[n - 1],
    mean,
    stddev,
  };
}

function fmtNs(ns) {
  if (ns == null) return "-";
  if (ns < 1000) return `${ns}ns`;
  if (ns < 1_000_000) return `${(ns / 1000).toFixed(ns < 10_000 ? 1 : 0)}us`;
  return `${(ns / 1_000_000).toFixed(2)}ms`;
}

function statusGlyph(slot, useColor = true) {
  if (!slot) return useColor ? `${c.gray}- ${c.reset}` : "- ";
  switch (slot.status) {
    case "ok": return useColor ? `${c.green}✓${c.reset}` : "✓";
    case "breach": return useColor ? `${c.red}✗${c.reset}` : "✗";
    case "missing": return useColor ? `${c.yellow}∅${c.reset}` : "∅";
    case "parse-error": return useColor ? `${c.red}!${c.reset}` : "!";
    default: return useColor ? `${c.gray}?${c.reset}` : "?";
  }
}

function printTable(reports) {
  // Column widths: recipe slug, rust status+p99, java status+p99, worst-stage.
  const headers = ["Recipe", "Rust", "p99", "Java", "p99", "Worst stage"];
  const rows = [headers];
  for (const r of reports) {
    const rustWorst = r.rust?.stages?.filter((s) => !s.baseline).sort((a, b) => b.p99 - a.p99)[0];
    const javaWorst = r.java?.stages?.filter((s) => !s.baseline).sort((a, b) => b.p99 - a.p99)[0];
    const overallWorst =
      (rustWorst?.p99 ?? 0) > (javaWorst?.p99 ?? 0) ? rustWorst : javaWorst;
    rows.push([
      r.slug,
      statusGlyph(r.rust, false),
      r.rust?.worstP99 != null ? fmtNs(r.rust.worstP99) : "-",
      statusGlyph(r.java, false),
      r.java?.worstP99 != null ? fmtNs(r.java.worstP99) : "-",
      overallWorst ? `${overallWorst.stage} (${(overallWorst.p99 / 1000).toFixed(1)}us)` : "-",
    ]);
  }
  const widths = headers.map((_, i) => Math.max(...rows.map((row) => String(row[i]).length)));
  for (let r = 0; r < rows.length; r++) {
    const isHeader = r === 0;
    const isSep = r === 1;
    const line = rows[r].map((cell, i) => String(cell).padEnd(widths[i])).join("  ");
    if (isHeader) console.log(`  ${c.bold}${line}${c.reset}`);
    else {
      // Re-tint status glyph + breach worst p99 with colour.
      const cells = rows[r].map((cell, i) => String(cell).padEnd(widths[i]));
      const rec = reports[r - 1];
      const tint = (s, slot) =>
        slot?.status === "breach"
          ? `${c.red}${s}${c.reset}`
          : slot?.status === "ok"
            ? `${c.green}${s}${c.reset}`
            : slot?.status === "missing"
              ? `${c.yellow}${s}${c.reset}`
              : `${c.gray}${s}${c.reset}`;
      cells[1] = tint(cells[1], rec.rust);
      cells[3] = tint(cells[3], rec.java);
      console.log(`  ${cells.join("  ")}`);
    }
    if (isHeader) console.log(`  ${c.dim}${"-".repeat(widths.reduce((a, b) => a + b + 2, 0))}${c.reset}`);
  }
}

async function writeMarkdownReport(path, reports) {
  const lines = [];
  lines.push("# Cookbook bench report");
  lines.push("");
  lines.push(`Generated ${new Date().toISOString()}`);
  lines.push(`Threshold: p99 < ${(threshold / 1000).toFixed(0)}us`);
  lines.push("");
  lines.push("| Recipe | Rust | Rust p99 | Java | Java p99 | Worst stage |");
  lines.push("|---|---|---|---|---|---|");
  for (const r of reports) {
    const rustGlyph = statusGlyph(r.rust, false);
    const javaGlyph = statusGlyph(r.java, false);
    const rustP99 = r.rust?.worstP99 != null ? fmtNs(r.rust.worstP99) : "-";
    const javaP99 = r.java?.worstP99 != null ? fmtNs(r.java.worstP99) : "-";
    const rustWorst = r.rust?.stages?.filter((s) => !s.baseline).sort((a, b) => b.p99 - a.p99)[0];
    const javaWorst = r.java?.stages?.filter((s) => !s.baseline).sort((a, b) => b.p99 - a.p99)[0];
    const worst = (rustWorst?.p99 ?? 0) > (javaWorst?.p99 ?? 0) ? rustWorst : javaWorst;
    lines.push(`| ${r.slug} | ${rustGlyph} | ${rustP99} | ${javaGlyph} | ${javaP99} | ${worst ? `\`${worst.stage}\` (${(worst.p99 / 1000).toFixed(1)}us)` : "-"} |`);
  }
  lines.push("");
  lines.push("## Per-stage breakdown");
  lines.push("");
  for (const r of reports) {
    if ((r.rust?.stages?.length ?? 0) === 0 && (r.java?.stages?.length ?? 0) === 0) continue;
    lines.push(`### ${r.slug}`);
    lines.push("");
    lines.push("| Lang | Workload | Stage | p50 | p99 | p999 | max | baseline |");
    lines.push("|---|---|---|---|---|---|---|---|");
    for (const lang of ["rust", "java"]) {
      const slot = r[lang];
      if (!slot?.stages) continue;
      for (const s of slot.stages) {
        const flag = s.breach ? " ⚠" : "";
        const baseline = s.baseline ? "yes" : "";
        lines.push(`| ${lang} | ${s.workload ?? "-"} | \`${s.stage}\`${flag} | ${fmtNs(s.p50)} | ${fmtNs(s.p99)} | ${fmtNs(s.p999)} | ${fmtNs(s.max)} | ${baseline} |`);
      }
    }
    lines.push("");
  }
  await writeFile(path, lines.join("\n"));
}

async function writeJsonReport(path, reports) {
  const out = {
    generated: new Date().toISOString(),
    threshold_ns: threshold,
    totals: { ok: totalOk, breach: totalBreaches, missing: totalMissing },
    recipes: reports.map((r) => ({
      slug: r.slug,
      rust: {
        status: r.rust?.status ?? "missing",
        worst_p99_ns: r.rust?.worstP99 ?? null,
        stages: r.rust?.stages ?? [],
      },
      java: {
        status: r.java?.status ?? "missing",
        worst_p99_ns: r.java?.worstP99 ?? null,
        stages: r.java?.stages ?? [],
      },
    })),
  };
  await writeFile(path, JSON.stringify(out, null, 2));
}
