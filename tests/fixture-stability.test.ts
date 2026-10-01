import { afterAll, describe, test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-fixture-stability-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

// The committed fixtures are generated, not hand-made, and every kind in
// tools/make-fixture.py shares its layout and PDF-emission code. Without
// this, a refactor for one kind could silently change screenplay.pdf and
// every integration test would quietly start asserting against different
// input. (torture.pdf is not pinned here; its placement is checked through
// --emit-layout instead.) The two picture fixtures also notice if the
// emitter stops being byte-exact about binary streams, but only images.pdf
// has binary streams that come from the HOST zlib; jpx.pdf's J2K codestream
// is copied through verbatim and involves no compressor.
//
// TRAP, if images.pdf fails here and the others pass: its bytes include
// deflate output from the HOST zlib, and the format pins only what
// decompresses back out, not how it was packed. Any zlib build whose deflate
// output differs (zlib-ng, a forked or hardware-accelerated zlib) packs the
// same pixels into different bytes. So a binary inequality on this fixture
// ALONE points at the compressor, not at make-fixture.py's emitter --
// decompress both sides and compare the pixels before believing the
// generator changed. What has been checked: images.pdf was regenerated
// byte-identically on Linux with zlib 1.2.13 / Python 3.11 on 2026-10-01;
// the Mac it was authored on is zlib 1.2.12. Deliberately left as a byte
// comparison anyway: the whole point is that a committed fixture is a fixed
// input, and a looser check here would stop catching real emitter drift.
//
// python3 is a REAL dependency of this suite, deliberately. No
// skip-if-missing guard: this repo already fixed the case where integration
// tests self-skipped and therefore never ran in CI. A test that quietly
// skips reads as coverage while providing none.
describe('committed fixtures regenerate byte-identically', () => {
  const COMMITTED = {
    screenplay: 'tests/fixtures/screenplay.pdf',
    prose: 'tests/fixtures/prose.pdf',
    blank: 'tests/fixtures/blank-pages.pdf',
    // The invented feature the README and site pictures show. Regenerated
    // byte for byte like the others, so a generator refactor cannot quietly
    // change what the pictures are of.
    demo: 'tests/fixtures/field-station.pdf',
    images: 'tests/fixtures/images.pdf',
    // jpx.pdf has no deflate in it at all -- its one picture is a committed
    // JPEG2000 codestream copied through verbatim -- so unlike images.pdf it
    // cannot drift with the host zlib, and a failure here is the emitter.
    jpx: 'tests/fixtures/jpx.pdf',
  } as const;

  for (const [kind, committed] of Object.entries(COMMITTED)) {
    test(kind, () => {
      const dir = mkdtempSync(join(SCRATCH, 'fixture-'));
      const out = join(dir, `${kind}.pdf`);
      const run = spawnSync('python3', ['tools/make-fixture.py', kind, out], {
        encoding: 'utf8',
      });
      expect(run.stderr).toBe('');
      expect(run.status).toBe(0);
      expect(readFileSync(out)).toEqual(readFileSync(committed));
    });
  }
});
