// TRANSITIONAL, deleted when sp-fountain's tokenizer lands (see
// tests/common/mod.rs): records what the TypeScript makes of the inline
// Fountain sources sp-fdx's tests use, the way tools/oracle/dump.ts records
// whole scripts. Each source keeps fountain-js's tokens and the TypeScript
// writer's own FDX of it, so the tests also hold the Rust writer to the
// TypeScript byte for byte.
//
//   SP_FDX_RECORD=1 cargo test -p sp-fdx   # notes the sources it lacks
//   bun rust/crates/sp-fdx/tests/data/record-tokens.ts
//
// Every source, old and new, is recorded afresh each run (keys sorted), so
// the store always holds what the TypeScript makes today; the pending file
// is then removed.
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { Fountain } from 'fountain-js';
import { fountainToFdx } from '../../../../../src/fdx/fdx';

const DIR = import.meta.dir;
const STORE = join(DIR, 'fountain-tokens.json');
const PENDING = join(DIR, 'pending-sources.jsonl');

const sources = new Set<string>(existsSync(STORE) ? Object.keys(JSON.parse(readFileSync(STORE, 'utf8'))) : []);
if (existsSync(PENDING)) {
  for (const line of readFileSync(PENDING, 'utf8').split('\n')) {
    if (line.trim() !== '') sources.add(JSON.parse(line));
  }
  rmSync(PENDING);
}
// fountain-js keeps one piece of state between parses: a static
// `Lexer.lastLineWasDualDialogue`, left true when a script's FIRST speech is
// the right half of a dual exchange (`NAME ^`). The next parse then opens a
// dual exchange around its LAST speech, and that speech can vanish from a
// book. So every parse here starts from a plain speech's state, the state
// a fresh process starts in, or one case's leftovers would decide the next.
const fresh = () => new Fountain().parse('BOB\nHi.\n', true);
const store: Record<string, unknown> = {};
for (const source of [...sources].sort()) {
  fresh();
  const tokens = new Fountain().parse(source, true).tokens;
  fresh();
  store[source] = { tokens, fdx: fountainToFdx(source) };
}
writeFileSync(STORE, JSON.stringify(store, null, 2) + '\n');
process.stdout.write(`${sources.size} sources recorded\n`);
