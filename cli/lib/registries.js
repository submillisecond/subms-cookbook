// Registry fetchers. Pure functions; the fetch implementation is injected
// so tests can pass a stub without monkey-patching globals.

const UA = '@submillisecond/subms CLI';

// crates.io: the published subms recipe crates plus the harness. The set grows
// when new subms recipes publish; add the name here when that happens. Excludes
// non-recipe subms crates (the growler catalog product, the cookbook meta-crate).
export const KNOWN_CRATES = Object.freeze([
  'subms',
  'subms-adaptive-radix-tree',
  'subms-arena-allocator',
  'subms-block-cache',
  'subms-bloom-filter',
  'subms-count-min-sketch',
  'subms-cuckoo-filter',
  'subms-hdr-histogram',
  'subms-hyperloglog',
  'subms-lsm-tree',
  'subms-merge-iterator',
  'subms-mpsc-queue',
  'subms-otel',
  'subms-rate-limiter',
  'subms-segment-reader',
  'subms-spsc-ring-buffer',
  'subms-stats',
  'subms-timer-wheel',
  'subms-treap',
]);

// Maven Central: these are the groupIds we own. search.maven.org returns
// all artefacts under each.
export const KNOWN_GROUPS = Object.freeze([
  'com.submillisecond',
  'com.submillisecond.recipes',
  'com.submillisecond.primers',
]);

/**
 * Fetch metadata for one crate from crates.io.
 *
 * Returns `{ name, latest, versions: string[] }` or `{ name, error }`.
 * Network errors do not throw - they degrade gracefully so a partial
 * report is still useful when one registry is flaky.
 */
export async function fetchCrate(name, { fetch = globalThis.fetch } = {}) {
  const url = `https://crates.io/api/v1/crates/${encodeURIComponent(name)}`;
  try {
    const r = await fetch(url, { headers: { 'User-Agent': UA, Accept: 'application/json' } });
    if (!r.ok) return { name, error: `HTTP ${r.status}` };
    const json = await r.json();
    const versions = (json.versions ?? [])
      .filter(v => !v.yanked)
      .map(v => v.num);
    const latest = json.crate?.max_stable_version ?? json.crate?.max_version ?? versions[0] ?? null;
    return { name, latest, versions };
  } catch (e) {
    return { name, error: e.message ?? String(e) };
  }
}

/**
 * Fetch all (group, artifact, version) rows under one Maven groupId from
 * the public Solr search API. Returns `{ groupId, entries: [{a, v}, ...] }`
 * or `{ groupId, error }`.
 */
export async function fetchMavenGroup(groupId, { fetch = globalThis.fetch } = {}) {
  const q   = `g:"${groupId}"`;
  const url = `https://search.maven.org/solrsearch/select?q=${encodeURIComponent(q)}&core=gav&rows=200&wt=json`;
  try {
    const r = await fetch(url, { headers: { 'User-Agent': UA, Accept: 'application/json' } });
    if (!r.ok) return { groupId, error: `HTTP ${r.status}` };
    const json = await r.json();
    const docs = json.response?.docs ?? [];
    const entries = docs.map(d => ({ a: d.a, v: d.v }));
    // Sort by artifact then by version (string-sort is good enough for
    // semver-shaped values; Solr already returned ordered but we want
    // stable output regardless).
    entries.sort((x, y) => x.a.localeCompare(y.a) || x.v.localeCompare(y.v));
    return { groupId, entries };
  } catch (e) {
    return { groupId, error: e.message ?? String(e) };
  }
}

/**
 * Fetch the whole subms ecosystem in parallel.
 *
 * @param {object} opts
 * @param {boolean} opts.rust   include crates.io section (default true)
 * @param {boolean} opts.java   include Maven Central section (default true)
 * @param {typeof fetch} opts.fetch  injected fetch (for tests)
 */
export async function listAll({ rust = true, java = true, fetch = globalThis.fetch } = {}) {
  const cratesP = rust
    ? Promise.all(KNOWN_CRATES.map(n => fetchCrate(n, { fetch })))
    : Promise.resolve([]);

  const mavenP = java
    ? Promise.all(KNOWN_GROUPS.map(g => fetchMavenGroup(g, { fetch })))
    : Promise.resolve([]);

  const [crates, maven] = await Promise.all([cratesP, mavenP]);
  return { crates, maven };
}
