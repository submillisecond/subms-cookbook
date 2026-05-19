#!/usr/bin/env node
// Thin shebang entry: delegates everything to lib/index.js so the public
// programmatic API (`import { run } from '@submillisecond/subms'`) and the
// CLI share the exact same code path.

import { run } from '../lib/index.js';

const exitCode = await run(process.argv.slice(2), { out: process.stdout, err: process.stderr });
process.exit(exitCode);
