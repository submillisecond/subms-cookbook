// Public surface for the @submillisecond/subms CLI.
//
// `run(argv, io)` is the single entry point. The shebang at bin/subms.js
// delegates straight to it; tests and downstream programmatic consumers
// import it directly.

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { listAll } from './registries.js';

// Resolve package.json next to lib/ at runtime, not at build time, so the
// version always matches what's actually shipped in the published tarball.
const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_PATH  = join(__dirname, '..', 'package.json');
const PKG       = JSON.parse(await readFile(PKG_PATH, 'utf8'));
export const VERSION = PKG.version;

const HELP = `\
${PKG.name} ${VERSION}

  Discover the subms cookbook ecosystem - lists every published recipe
  and its current versions on crates.io and Maven Central.

Usage:
  subms                 List the whole ecosystem (default).
  subms list            Same as above.
  subms list --json     Machine-readable JSON instead of the formatted report.
  subms list --rust     Only show crates.io.
  subms list --java     Only show Maven Central.

Flags:
  -h, --help            Show this message.
  -V, --version         Print the CLI version.
  --no-color            Disable ANSI colour.

Docs: ${PKG.homepage}
`;

/**
 * Entry point. Parses argv, dispatches a subcommand, prints to the
 * supplied io.out / io.err. Returns the intended process exit code.
 *
 * Designed to be testable: caller passes string-streams or sinks; the
 * function itself never touches process.exit or process.stdout directly.
 */
export async function run(argv, { out = process.stdout, err = process.stderr, fetch = globalThis.fetch } = {}) {
  const args = parseArgs(argv);

  if (args.help)    { out.write(HELP);                 return 0; }
  if (args.version) { out.write(`${VERSION}\n`);       return 0; }

  // Default subcommand and the only one for now is `list`.
  switch (args.command) {
    case undefined:
    case 'list':
    case 'ls':
      return runList(args, { out, err, fetch });
    default:
      err.write(`subms: unknown command '${args.command}'. See 'subms --help'.\n`);
      return 1;
  }
}

async function runList(args, { out, err, fetch }) {
  // If both --rust and --java were passed (or neither), include both.
  const rust = !args.javaOnly;
  const java = !args.rustOnly;

  let data;
  try {
    data = await listAll({ rust, java, fetch });
  } catch (e) {
    err.write(`subms: failed to query registries: ${e.message ?? e}\n`);
    return 2;
  }

  if (args.json) {
    out.write(JSON.stringify(data, null, 2) + '\n');
    return 0;
  }

  const c = ansi(!args.noColor);
  printReport(data, { rust, java, out, c });
  return 0;
}

// --- argv parsing ----------------------------------------------------

function parseArgs(argv) {
  const args = {
    command:   undefined,
    help:      false,
    version:   false,
    json:      false,
    noColor:   process.env.NO_COLOR != null,
    rustOnly:  false,
    javaOnly:  false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    switch (a) {
      case '-h': case '--help':     args.help = true;     break;
      case '-V': case '--version':  args.version = true;  break;
      case '--json':                args.json = true;     break;
      case '--no-color':            args.noColor = true;  break;
      case '--rust':                args.rustOnly = true; break;
      case '--java':                args.javaOnly = true; break;
      default:
        if (a.startsWith('-')) {
          // unknown flag - leave for the caller to detect via help text
          continue;
        }
        if (args.command == null) args.command = a;
    }
  }
  return args;
}

// --- formatted report ------------------------------------------------

function ansi(enabled) {
  if (!enabled) return { reset: '', bold: '', dim: '', cyan: '', yellow: '', green: '', red: '' };
  return {
    reset: '\x1b[0m', bold: '\x1b[1m', dim: '\x1b[2m',
    cyan: '\x1b[36m', yellow: '\x1b[33m', green: '\x1b[32m', red: '\x1b[31m',
  };
}

function printReport(data, { rust, java, out, c }) {
  if (rust) {
    out.write(`\n${c.bold}${c.cyan}=== crates.io ===${c.reset}\n`);
    for (const row of data.crates) {
      if (row.error) {
        out.write(`  ${row.name.padEnd(22)} ${c.yellow}(${row.error})${c.reset}\n`);
        continue;
      }
      const ver   = row.latest ? `v${row.latest}` : '<no releases>';
      const count = row.versions.length;
      const url   = `https://crates.io/crates/${row.name}`;
      out.write(`  ${row.name.padEnd(22)} ${c.green}${ver.padEnd(9)}${c.reset} ${String(count).padStart(2)} version(s)   ${c.dim}${url}${c.reset}\n`);
    }
  }

  if (java) {
    out.write(`\n${c.bold}${c.cyan}=== Maven Central ===${c.reset}\n`);
    let anyEntry = false;
    for (const grp of data.maven) {
      if (grp.error) {
        out.write(`  ${grp.groupId} ${c.yellow}(${grp.error})${c.reset}\n`);
        continue;
      }
      if (grp.entries.length === 0) continue;
      anyEntry = true;
      for (const e of grp.entries) {
        const coord = `${grp.groupId}:${e.a}`;
        out.write(`  ${coord.padEnd(50)}  ${c.green}${e.v}${c.reset}\n`);
      }
    }
    if (!anyEntry && !data.maven.some(g => g.error)) {
      out.write(`  ${c.dim}(nothing indexed yet - search.maven.org lags PUBLISHED by 10-30 min)${c.reset}\n`);
    }
  }

  out.write('\n');
}
