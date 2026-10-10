// The committed fixtures' EPUBs, converted once per test file and shared by
// the AZW3 tests: field-station.pdf (the 18-page feature), images.pdf (two
// pictures, one of them the cover) and torture.pdf (every content-driven
// registry behavior, dual dialogue tables among them).
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { convertPdf } from '../../src/convert';

export const FIXTURES = join(import.meta.dir, '..', 'fixtures');
export type FixtureName = 'field-station' | 'images' | 'torture' | 'screenplay';

const cache = new Map<FixtureName, Promise<Uint8Array>>();

export function fixtureEpub(name: FixtureName): Promise<Uint8Array> {
  let p = cache.get(name);
  if (!p) {
    p = convertPdf(new Uint8Array(readFileSync(join(FIXTURES, `${name}.pdf`))), { force: true }).then((r) => r.epub);
    cache.set(name, p);
  }
  return p;
}
