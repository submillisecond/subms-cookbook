#!/usr/bin/env node
// Cross-repo test + coverage audit.
//
// Walks the central subms harness and every cookbook recipe; counts test
// fns + LOC of source vs tests, optionally runs jacoco for Java coverage.
// Output: one row per (component, lang) - good for spotting gaps.

import { readdir, readFile, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { spawn } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const COOKBOOK = join(__dirname, "..");
const ORG_ROOT = join(COOKBOOK, "..");
const SUBMS = join(ORG_ROOT, "subms");

const args = process.argv.slice(2);
const flag = (n) => args.includes(`--${n}`);
const wantJacoco = flag("jacoco");
const wantLlvmCov = flag("llvm-cov");

function run(cmd, cmdArgs, opts) {
  return new Promise((resolve) => {
    const p = spawn(cmd, cmdArgs, {
      ...opts,
      stdio: ["ignore", "pipe", "pipe"],
      shell: process.platform === "win32",
    });
    let out = "";
    let err = "";
    p.stdout.on("data", (b) => (out += b.toString()));
    p.stderr.on("data", (b) => (err += b.toString()));
    p.on("close", (code) => resolve({ code, out, err }));
    p.on("error", (e) => resolve({ code: -1, out: "", err: String(e) }));
  });
}

async function walkFiles(root, predicate) {
  const out = [];
  async function rec(dir) {
    let entries;
    try { entries = await readdir(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (e.isDirectory()) {
        // skip build dirs
        if (["target", "node_modules", "dist", "build", ".idea", ".vscode", ".git"].includes(e.name)) continue;
        await rec(join(dir, e.name));
      } else if (e.isFile() && predicate(e.name, join(dir, e.name))) {
        out.push(join(dir, e.name));
      }
    }
  }
  await rec(root);
  return out;
}

async function loc(file) {
  try {
    const s = await readFile(file, "utf8");
    return s.split("\n").length;
  } catch {
    return 0;
  }
}

async function countMatches(file, pattern) {
  try {
    const s = await readFile(file, "utf8");
    const m = s.match(pattern);
    return m ? m.length : 0;
  } catch {
    return 0;
  }
}

async function auditRust(name, rustRoot) {
  if (!existsSync(rustRoot)) return null;
  const srcFiles = await walkFiles(join(rustRoot, "src"), (n) => n.endsWith(".rs"));
  const testFiles = existsSync(join(rustRoot, "tests"))
    ? await walkFiles(join(rustRoot, "tests"), (n) => n.endsWith(".rs"))
    : [];
  let srcLoc = 0, testLoc = 0, testCount = 0;
  // For src files: split LOC into "production" and "inline-test" by
  // looking for the `mod tests` / `#[cfg(test)] mod` boundary. Avoids
  // the misleading 6% ratio on subms (where tests are inline).
  for (const f of srcFiles) {
    const s = await readFile(f, "utf8").catch(() => "");
    const lines = s.split("\n");
    const testStart = lines.findIndex(
      (l) => /^#\[cfg\(test\)\]/.test(l) || /^\s*mod tests\b/.test(l),
    );
    if (testStart === -1) {
      srcLoc += lines.length;
    } else {
      srcLoc += testStart;
      testLoc += lines.length - testStart;
    }
    testCount += (s.match(/#\[test\]/g) ?? []).length;
  }
  for (const f of testFiles) {
    testLoc += await loc(f);
    testCount += await countMatches(f, /#\[test\]/g);
  }
  return {
    component: name,
    lang: "rust",
    srcFiles: srcFiles.length,
    srcLoc,
    testFiles: testFiles.length,
    testLoc,
    testCount,
    coveragePct: null,
  };
}

async function auditJava(name, javaRoot) {
  if (!existsSync(javaRoot)) return null;
  const srcDir = join(javaRoot, "src", "main", "java");
  const testDir = join(javaRoot, "src", "test", "java");
  const srcFiles = existsSync(srcDir)
    ? await walkFiles(srcDir, (n) => n.endsWith(".java"))
    : [];
  const testFiles = existsSync(testDir)
    ? await walkFiles(testDir, (n) => n.endsWith(".java"))
    : [];
  let srcLoc = 0, testLoc = 0, testCount = 0;
  for (const f of srcFiles) srcLoc += await loc(f);
  for (const f of testFiles) {
    testLoc += await loc(f);
    testCount += await countMatches(f, /@Test\b/g);
  }
  let coveragePct = null;
  if (wantJacoco) {
    const r = await run("mvn", ["-q", "test", "jacoco:report", "-Dskip.gpg=true"], { cwd: javaRoot });
    if (r.code === 0) {
      const csv = join(javaRoot, "target", "site", "jacoco", "jacoco.csv");
      if (existsSync(csv)) {
        const lines = (await readFile(csv, "utf8")).split("\n").slice(1).filter(Boolean);
        let missed = 0, covered = 0;
        for (const l of lines) {
          const cols = l.split(",");
          // Standard jacoco CSV columns:
          // GROUP,PACKAGE,CLASS,INSTRUCTION_MISSED,INSTRUCTION_COVERED,BRANCH_MISSED,...,LINE_MISSED(7),LINE_COVERED(8)
          missed += parseInt(cols[7] ?? "0", 10) || 0;
          covered += parseInt(cols[8] ?? "0", 10) || 0;
        }
        if (missed + covered > 0) coveragePct = (covered * 100) / (missed + covered);
      }
    }
  }
  return {
    component: name,
    lang: "java",
    srcFiles: srcFiles.length,
    srcLoc,
    testFiles: testFiles.length,
    testLoc,
    testCount,
    coveragePct,
  };
}

const rows = [];

console.log("auditing...");
rows.push(await auditRust("subms (harness)", join(SUBMS, "rust")));
rows.push(await auditJava("subms (harness)", join(SUBMS, "java")));

const recipesDir = join(COOKBOOK, "recipes");
const entries = await readdir(recipesDir, { withFileTypes: true });
for (const e of entries.sort((a, b) => a.name.localeCompare(b.name))) {
  if (!e.isDirectory() || !e.name.startsWith("subms-")) continue;
  rows.push(await auditRust(e.name, join(recipesDir, e.name, "rust")));
  rows.push(await auditJava(e.name, join(recipesDir, e.name, "java")));
}

const valid = rows.filter(Boolean);
const headers = ["Component", "Lang", "Src files", "Src LOC", "Test files", "Test LOC", "Tests", "Test/Src", "Cov %"];
const data = valid.map((r) => [
  r.component,
  r.lang,
  r.srcFiles,
  r.srcLoc,
  r.testFiles,
  r.testLoc,
  r.testCount,
  r.srcLoc === 0 ? "-" : ((r.testLoc / r.srcLoc) * 100).toFixed(0) + "%",
  r.coveragePct == null ? "-" : r.coveragePct.toFixed(1) + "%",
]);
const widths = headers.map((h, i) =>
  Math.max(h.length, ...data.map((row) => String(row[i]).length)),
);
function row(cells) {
  return cells.map((c, i) => String(c).padEnd(widths[i])).join("  ");
}
console.log();
console.log(row(headers));
console.log("-".repeat(widths.reduce((a, b) => a + b + 2, 0)));
for (const r of data) console.log(row(r));

const totalTests = valid.reduce((a, r) => a + r.testCount, 0);
const totalSrc = valid.reduce((a, r) => a + r.srcLoc, 0);
const totalTestLoc = valid.reduce((a, r) => a + r.testLoc, 0);
console.log();
console.log(`Totals: ${totalTests} tests, ${totalSrc} src LOC, ${totalTestLoc} test LOC, ratio ${((totalTestLoc / Math.max(1, totalSrc)) * 100).toFixed(0)}%`);

// Real bars: >= 10 tests (depth) AND, when measurable, >= 90% line coverage.
// LOC ratio is a soft signal, not a bar.
const gaps = valid.filter(
  (r) => r.testCount < 10 || (r.coveragePct != null && r.coveragePct < 90),
);
if (gaps.length > 0) {
  console.log();
  console.log(`Coverage gaps (${gaps.length} components below the bars):`);
  for (const g of gaps) {
    const reasons = [];
    if (g.testCount < 10) reasons.push(`only ${g.testCount} tests (<10 depth bar)`);
    if (g.coveragePct != null && g.coveragePct < 90)
      reasons.push(`coverage ${g.coveragePct.toFixed(1)}% (<90% bar)`);
    console.log(`  ${g.component} (${g.lang}): ${reasons.join(", ")}`);
  }
} else {
  console.log();
  console.log(`All components above the >=10-test depth bar and >=90% coverage bar where measurable.`);
}
