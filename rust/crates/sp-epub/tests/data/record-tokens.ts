// TRANSITIONAL: deleted when sp-fountain's tokenizer lands, at which point
// the Rust tests tokenize their sources themselves.
//
// The ported tests (rust/crates/sp-epub/tests/*.rs) feed the renderer
// fountain-js tokens for small invented scripts. Until sp-fountain has a
// tokenizer, those tokens come from this recording: fountain-js run over
// every source the TypeScript tests parse, keyed by the source text.
//
//   bun test --preload ./rust/crates/sp-epub/tests/data/record-tokens.ts \
//     tests/epub.test.ts tests/epub-images.test.ts tests/options.test.ts
//
// writes rust/crates/sp-epub/tests/data/fountain-tokens.json.
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { Fountain } from 'fountain-js';

const OUT = join(import.meta.dir, 'fountain-tokens.json');
const seen = new Map<string, unknown>();
const parse = Fountain.prototype.parse;
Fountain.prototype.parse = function (this: Fountain, script: string, getTokens?: boolean) {
  const result = parse.call(this, script, getTokens);
  if (getTokens && !seen.has(script)) {
    seen.set(script, JSON.parse(JSON.stringify(result.tokens)));
    // Written on every new source: bun test does not reliably run a
    // preload's exit handler.
    const sorted = [...seen.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    writeFileSync(OUT, `${JSON.stringify(Object.fromEntries(sorted))}\n`);
  }
  return result;
};
