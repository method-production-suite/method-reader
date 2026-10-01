import { describe, test, expect, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  soleJson,
  checkVersion,
  checkConvertResult,
  checkJpxPicture,
  checkEpubBytes,
  checkDevices,
  smokeCli,
  type Runner,
} from '../tools/smoke-cli';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-smoke-cli-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const ok = (stdout: string) => ({ exitCode: 0, stdout, stderr: '' });

describe('soleJson', () => {
  test('accepts exactly one JSON object', () => {
    expect(soleJson('{"ok":true}\n', 'x')).toEqual({ ok: true });
  });

  test('rejects two objects, which is the --json contract breaking', () => {
    // The contract is ONE parseable object on stdout. Two would parse fine
    // if a checker only looked at the first line.
    expect(() => soleJson('{"ok":true}\n{"ok":false}\n', 'convert')).toThrow(/2 lines/);
  });

  test('rejects empty output and non-JSON', () => {
    expect(() => soleJson('', 'convert')).toThrow(/0 lines/);
    expect(() => soleJson('converting...\n', 'convert')).toThrow(/parseable JSON/);
  });
});

describe('checkVersion', () => {
  test('passes only on the exact expected line', () => {
    expect(() => checkVersion(ok('screepub 0.6.0\n'), '0.6.0')).not.toThrow();
  });

  test('a stale binary reporting the old version fails', () => {
    // The failure this exists for: an artifact built from a checkout that
    // was not the tagged one.
    expect(() => checkVersion(ok('screepub 0.5.4\n'), '0.6.0')).toThrow(/0\.5\.4/);
    // And a substring match must not save it.
    expect(() => checkVersion(ok('screepub 0.6.0-dirty\n'), '0.6.0')).toThrow();
    expect(() => checkVersion({ exitCode: 1, stdout: '', stderr: 'boom' }, '0.6.0')).toThrow(/exited 1/);
  });
});

describe('checkConvertResult', () => {
  const good = ok('{"ok":true,"epubPath":"/tmp/smoke.epub","pages":97}\n');

  test('passes on a real success payload', () => {
    expect(() => checkConvertResult(good, '/tmp/smoke.epub')).not.toThrow();
  });

  test('a reported failure fails, even at exit 0', () => {
    const failed = ok('{"ok":false,"error":{"code":"not-screenplay","message":"x"}}\n');
    expect(() => checkConvertResult(failed, '/tmp/smoke.epub')).toThrow(/not-screenplay/);
  });

  test('zero pages fails: an empty book is not a conversion', () => {
    const empty = ok('{"ok":true,"epubPath":"/tmp/smoke.epub","pages":0}\n');
    expect(() => checkConvertResult(empty, '/tmp/smoke.epub')).toThrow(/pages/);
    const none = ok('{"ok":true,"epubPath":"/tmp/smoke.epub"}\n');
    expect(() => checkConvertResult(none, '/tmp/smoke.epub')).toThrow(/pages/);
  });

  test('writing somewhere other than where we asked fails', () => {
    expect(() => checkConvertResult(good, '/tmp/other.epub')).toThrow(/other\.epub/);
  });

  test('a non-zero exit fails and carries stderr', () => {
    const crashed = { exitCode: 134, stdout: '', stderr: 'Segmentation fault' };
    expect(() => checkConvertResult(crashed, '/tmp/smoke.epub')).toThrow(/134/);
    expect(() => checkConvertResult(crashed, '/tmp/smoke.epub')).toThrow(/Segmentation/);
  });

  test('hands back the answer it vetted, for the next question about it', () => {
    expect(checkConvertResult(good, '/tmp/smoke.epub')).toEqual({
      ok: true, epubPath: '/tmp/smoke.epub', pages: 97,
    });
  });
});

describe('checkJpxPicture', () => {
  test('one picture carried is a pass', () => {
    expect(() => checkJpxPicture({ ok: true, imageCount: 1, warnings: [] })).not.toThrow();
  });

  test('a decoder that did not load fails, though the conversion succeeded', () => {
    // The real shape of the failure (tests/jpx-fixture.test.ts pins it): ok,
    // exit 0, no imageCount at all, and the cause only in the warnings. The
    // warnings must reach the message, because on a Windows runner they are
    // all anyone will see.
    const lost = {
      ok: true,
      warnings: [
        '1 picture could not be read out of the PDF, so it was left out.',
        'Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read.',
      ],
    };
    expect(() => checkJpxPicture(lost)).toThrow(/JPEG 2000 fixture carried no picture/);
    expect(() => checkJpxPicture(lost)).toThrow(/could not unpack its picture decoders/);
  });

  test('any count but exactly one fails: the fixture holds one picture', () => {
    expect(() => checkJpxPicture({ ok: true, imageCount: 0 })).toThrow(/carried 0/);
    expect(() => checkJpxPicture({ ok: true, imageCount: 2 })).toThrow(/carried 2/);
    expect(() => checkJpxPicture({ ok: true, imageCount: '1' })).toThrow();
  });
});

describe('smokeCli', () => {
  // A stand-in for the binary: smokeCli only checks that the path exists,
  // and the fake runner below answers in its place.
  const BINARY = join(SCRATCH, 'screepub');
  const SCREENPLAY = join(SCRATCH, 'screenplay.pdf');
  const JPX = join(SCRATCH, 'jpx.pdf');
  for (const p of [BINARY, SCREENPLAY, JPX]) writeFileSync(p, '');

  /** Answers every command the way a working binary would, except that the
   *  JPEG 2000 conversion answers `jpxAnswer`. Writes a zip header wherever
   *  -o points, as a real conversion would, and records every argv. */
  function fakeBinary(jpxAnswer: Record<string, unknown>) {
    const calls: string[][] = [];
    const run: Runner = (argv) => {
      calls.push(argv);
      if (argv[1] === '--version') return ok('screepub 0.6.0\n');
      if (argv[1] === 'devices') return ok('{"ok":true,"devices":[]}\n');
      const epub = argv[argv.indexOf('-o') + 1]!;
      writeFileSync(epub, new Uint8Array([0x50, 0x4b, 0x03, 0x04]));
      const extra = argv[1] === JPX ? jpxAnswer : {};
      return ok(`${JSON.stringify({ ok: true, epubPath: epub, pages: 3, ...extra })}\n`);
    };
    return { run, calls };
  }

  test('converts the JPEG 2000 fixture as well as the script, and passes on one picture', () => {
    const work = mkdtempSync(join(SCRATCH, 'pass-'));
    const { run, calls } = fakeBinary({ imageCount: 1 });
    expect(() => smokeCli(BINARY, SCREENPLAY, JPX, work, '0.6.0', run)).not.toThrow();
    const jpxCall = calls.find((c) => c[1] === JPX);
    expect(jpxCall).toEqual([BINARY, JPX, '-o', join(work, 'smoke-jpx.epub'), '--no-fountain', '--json']);
  });

  test('fails when the JPEG 2000 picture did not come through', () => {
    const work = mkdtempSync(join(SCRATCH, 'fail-'));
    const { run } = fakeBinary({
      warnings: ['Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read.'],
    });
    expect(() => smokeCli(BINARY, SCREENPLAY, JPX, work, '0.6.0', run)).toThrow(/JPEG 2000/);
  });

  test('a missing JPEG 2000 fixture fails before anything runs', () => {
    const work = mkdtempSync(join(SCRATCH, 'missing-'));
    const { run, calls } = fakeBinary({ imageCount: 1 });
    expect(() => smokeCli(BINARY, SCREENPLAY, join(SCRATCH, 'nope.pdf'), work, '0.6.0', run))
      .toThrow(/no JPEG 2000 fixture/);
    expect(calls).toEqual([]);
  });
});

describe('checkEpubBytes', () => {
  test('an EPUB is a zip container', () => {
    expect(() => checkEpubBytes(new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x14]))).not.toThrow();
  });

  test('a 0-byte or non-zip output fails', () => {
    // The CLI can report success and still have written nothing usable.
    expect(() => checkEpubBytes(new Uint8Array(0))).toThrow();
    expect(() => checkEpubBytes(new TextEncoder().encode('<html>'))).toThrow();
  });
});

describe('checkDevices', () => {
  test('an empty device list is a pass: the command ran', () => {
    expect(() => checkDevices(ok('{"ok":true,"devices":[]}\n'))).not.toThrow();
    expect(() => checkDevices(ok('{"ok":true,"devices":[{"id":"kindle"}]}\n'))).not.toThrow();
  });

  test('a missing array, a reported failure, or a crash all fail', () => {
    expect(() => checkDevices(ok('{"ok":true}\n'))).toThrow(/devices/);
    expect(() => checkDevices(ok('{"ok":false,"error":{"code":"internal"}}\n'))).toThrow(/internal/);
    expect(() => checkDevices({ exitCode: 1, stdout: '', stderr: 'no' })).toThrow(/exited 1/);
  });
});
