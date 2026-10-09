import { afterAll, describe, test, expect } from 'bun:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isWriteRefusal, mapConversionError } from '../src/cli-errors';

const ROOT = new URL('..', import.meta.url).pathname;
const FIXTURES = new URL('./fixtures/', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-'));
const FIXTURE_PDF = `${FIXTURES}screenplay.pdf`;

afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

async function runCli(args: string[]) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { stdout, stderr, exitCode };
}

describe('cli --version', () => {
  test('prints a semver and exits 0', async () => {
    const { stdout, exitCode } = await runCli(['--version']);
    expect(exitCode).toBe(0);
    expect(stdout.trim()).toMatch(/^screepub \d+\.\d+\.\d+/);
  });

  test('matches the version in package.json', async () => {
    const pkg = await Bun.file(`${ROOT}package.json`).json();
    const { stdout } = await runCli(['--version']);
    expect(stdout.trim()).toBe(`screepub ${pkg.version}`);
  });
});

// pdf.js warns about our build choices — the modern-build notice fires at
// import time, and the standard-font notice on every base-14 PDF, which is
// most screenplays. Neither is actionable for someone converting a script,
// and both look like errors. They belong behind --debug.
describe('cli quiets pdf.js internals', () => {
  test('a normal conversion prints nothing to stderr', async () => {
    const { stderr, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', `${SCRATCH}/quiet.epub`, '--no-fountain',
    ]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
  }, 60000);

  test('--debug lets them through', async () => {
    const { stderr, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', `${SCRATCH}/loud.epub`, '--no-fountain', '--debug',
    ]);
    expect(exitCode).toBe(0);
    expect(stderr).toContain('Warning');
  }, 60000);
});

describe('cli --json contract', () => {
  test('success emits a single machine-readable JSON object', async () => {
    const out = `${SCRATCH}/cli-json-test.epub`;
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', out, '--no-fountain', '--json',
    ]);
    expect(exitCode).toBe(0);
    const result = JSON.parse(stdout);
    expect(result.ok).toBe(true);
    expect(result.title).toBe('The Last Video Store');
    expect(result.author).toBe('A. N. Placeholder');
    expect(result.pages).toBe(5);
    expect(result.scenes).toBe(5);
    expect(result.epubPath).toBe(out);
    expect(result.fountainPath).toBeUndefined();
    expect(Array.isArray(result.topCharacters)).toBe(true);
    expect(result.topCharacters.slice(0, 3)).toEqual(['MARGO', 'DEV', 'NIECE']);
    expect(Array.isArray(result.warnings)).toBe(true);
  }, 60000);

  test('guard errors emit structured JSON with a code and exit 1', async () => {
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}prose.pdf`, '-o', `${SCRATCH}/nope.epub`, '--json',
    ]);
    expect(exitCode).toBe(1);
    const result = JSON.parse(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('not-screenplay');
    expect(result.error.message).toContain('--force');
  }, 60000);

  test('scanned PDFs report their own error code', async () => {
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}blank-pages.pdf`, '-o', `${SCRATCH}/nope2.epub`, '--json',
    ]);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error.code).toBe('scanned');
  }, 60000);

  // --json is the app's only channel: EVERY exit in that mode must be a
  // parseable JSON object on stdout, or the app shows the user a raw
  // stack trace (or "engine produced no output").

  test('an unknown flag with --json still emits JSON, not a stack trace', async () => {
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '--json', '--no-such-flag',
    ]);
    expect(exitCode).toBe(1);
    const result = JSON.parse(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('usage');
  });

  test('no input with --json emits JSON, not help text on stdout', async () => {
    const { stdout, exitCode } = await runCli(['--json']);
    expect(exitCode).toBe(1);
    const result = JSON.parse(stdout);
    expect(result.ok).toBe(false);
    expect(result.error.code).toBe('usage');
  });

  test('two inputs is a usage error, not an internal one', async () => {
    const { stdout } = await runCli([
      `${FIXTURES}screenplay.pdf`, `${FIXTURES}prose.pdf`, '--json',
    ]);
    expect(JSON.parse(stdout).error.code).toBe('usage');
  });

  test('a directory named .pdf reports unreadable', async () => {
    const dir = `${SCRATCH}/a-folder.pdf`;
    mkdirSync(dir);
    const { stdout, exitCode } = await runCli([dir, '--json']);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error.code).toBe('unreadable');
  });

  test('a malformed options file reports bad-options', async () => {
    const opts = `${SCRATCH}/bad-options.json`;
    writeFileSync(opts, 'not json');
    const { stdout } = await runCli([
      `${FIXTURES}screenplay.pdf`, '--options', opts, '--json',
    ]);
    expect(JSON.parse(stdout).error.code).toBe('bad-options');
  });

  test('a corrupt PDF emits pure JSON on stdout, never a stack trace', async () => {
    const p = `${SCRATCH}/garbage.pdf`;
    writeFileSync(p, 'not a pdf at all');
    const { stdout, exitCode } = await runCli([p, '--json']);
    expect(exitCode).toBe(1);
    const result = JSON.parse(stdout); // throws if anything non-JSON leaked
    expect(result.ok).toBe(false);
    expect(typeof result.error.code).toBe('string');
  }, 60000);

  test('an unreadable-permissions file emits JSON with a code', async () => {
    const p = `${SCRATCH}/locked.pdf`;
    writeFileSync(p, 'x');
    chmodSync(p, 0o000);
    const { stdout, exitCode } = await runCli([p, '--json']);
    chmodSync(p, 0o644);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error.code).toBe('unreadable');
  });

  test('the success payload keys match the committed contract sample', async () => {
    // Every optional output is requested so every key is present. Until
    // piece F3 the Swift app's own checks decoded this same sample as the
    // consumer, so a key rename had to break one suite or the other; that
    // half went with the app, and this is the producer's pin against the
    // committed record of the keys.
    const out = `${SCRATCH}/contract.epub`;
    const { stdout } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', out, '--json', '--mobi', '--debug',
      '--preview-html', `${SCRATCH}/contract.html`,
    ]);
    const result = JSON.parse(stdout);
    const sample = await Bun.file(`${FIXTURES}engine-result-sample.json`).json();
    expect(Object.keys(result).sort()).toEqual(Object.keys(sample).sort());
  }, 60000);
});

// An output path whose folder is not there used to convert the whole script
// and then die inside node:fs: a raw ENOENT and a stack trace in a terminal,
// and code `internal` under --json, which the window heads "The engine did
// not answer". Now it is refused before anything is read or written, in the
// CLI's own one line, naming the folder. The folder is never made for the
// caller: `-o Docuemnts/x.epub` must not grow a Docuemnts folder.
/** The output paths a caller can name, each with a file name it takes. */
const OUTPUTS = [
  ['-o', 'x.epub'],
  ['--fountain', 'x.fountain'],
  ['--preview-html', 'x.html'],
  ['--fdx', 'x.fdx'],
] as const;

describe('an output folder that does not exist', () => {
  const missing = `${SCRATCH}/no-such-folder`;

  /** The run, with `flag` aimed into the missing folder. Under the other two
   * flags -o still names a real folder, so the book NOT being there proves
   * the refusal came before the conversion wrote anything. */
  function argsFor(flag: string, file: string, beside: string): string[] {
    const args = [FIXTURE_PDF, flag, `${missing}/${file}`];
    if (flag !== '-o') args.push('-o', beside);
    return args;
  }

  for (const [flag, file] of OUTPUTS) {
    test(`${flag}: one line naming the folder, nothing written`, async () => {
      const beside = `${SCRATCH}/missing-folder-text${flag}.epub`;
      const { stdout, stderr, exitCode } = await runCli(argsFor(flag, file, beside));
      expect(exitCode).toBe(1);
      expect(stdout).toBe('');
      const lines = stderr.trim().split('\n');
      expect(lines).toHaveLength(1);
      expect(lines[0]).toStartWith('screepub: ');
      expect(lines[0]).toContain(flag);
      expect(lines[0]).toContain(missing);
      expect(lines[0]).not.toContain('ENOENT');
      expect(lines[0]).not.toContain('—');
      expect(existsSync(missing)).toBe(false);
      expect(existsSync(beside)).toBe(false);
    }, 60000);

    test(`${flag} under --json: a usage answer naming the folder`, async () => {
      const beside = `${SCRATCH}/missing-folder-json${flag}.epub`;
      const { stdout, exitCode } = await runCli([...argsFor(flag, file, beside), '--json']);
      expect(exitCode).toBe(1);
      expect(stdout.trim().split('\n')).toHaveLength(1);
      const answer = JSON.parse(stdout);
      expect(answer.ok).toBe(false);
      expect(answer.error.code).toBe('usage');
      expect(answer.error.message).toContain(flag);
      expect(answer.error.message).toContain(missing);
      expect(existsSync(missing)).toBe(false);
      expect(existsSync(beside)).toBe(false);
    }, 60000);
  }

  test('a "folder" that is really a file is named as one', async () => {
    const notAFolder = `${SCRATCH}/plain-file.txt`;
    writeFileSync(notAFolder, 'not a folder');
    const { stdout, exitCode } = await runCli([FIXTURE_PDF, '-o', `${notAFolder}/x.epub`, '--json']);
    expect(exitCode).toBe(1);
    const answer = JSON.parse(stdout);
    expect(answer.error.code).toBe('usage');
    expect(answer.error.message).toContain(notAFolder);
    expect(answer.error.message).toContain('is a file');
  }, 60000);
});

// The rest of what can stop an output being written. A path that IS a folder
// is known before converting, so it is refused there like a missing folder.
// A write that fails anyway (no permission, a full disk, a folder sitting
// where a default output goes) used to end as a stack trace, or as code
// `internal` naming the hidden `.<name>.<pid>.tmp` the write goes through.
// It is now `unwritable`, in one line, naming the path the caller chose. Only
// the filesystem refusing gets that code: a throw that is a bug of ours stays
// `internal` (isWriteRefusal, below).
describe('an output that cannot be written', () => {
  for (const [flag, file] of OUTPUTS) {
    test(`${flag} naming a folder is refused before converting`, async () => {
      const folder = `${SCRATCH}/names-a-folder${flag}`;
      mkdirSync(folder);
      const beside = `${SCRATCH}/names-a-folder-beside${flag}.epub`;
      const args = [FIXTURE_PDF, flag, folder, '--json'];
      if (flag !== '-o') args.push('-o', beside);
      const { stdout, exitCode } = await runCli(args);
      expect(exitCode).toBe(1);
      const answer = JSON.parse(stdout);
      expect(answer.error.code).toBe('usage');
      expect(answer.error.message).toContain(flag);
      expect(answer.error.message).toContain(folder);
      expect(answer.error.message).toContain('is a folder');
      expect(existsSync(beside)).toBe(false);
    }, 60000);

    test(`${flag} into a folder with no write permission: unwritable, naming the path`, async () => {
      const locked = `${SCRATCH}/no-write${flag}`;
      mkdirSync(locked);
      chmodSync(locked, 0o555);
      const target = `${locked}/${file}`;
      const args = [FIXTURE_PDF, flag, target, '--json'];
      if (flag !== '-o') args.push('-o', `${SCRATCH}/no-write-beside${flag}.epub`);
      try {
        const { stdout, exitCode } = await runCli(args);
        expect(exitCode).toBe(1);
        expect(stdout.trim().split('\n')).toHaveLength(1);
        const answer = JSON.parse(stdout);
        expect(answer.ok).toBe(false);
        expect(answer.error.code).toBe('unwritable');
        expect(answer.error.message).toContain(target);
        expect(answer.error.message).toContain('permission denied');
        expect(answer.error.message).not.toContain('.tmp');
      } finally {
        chmodSync(locked, 0o755);
      }
    }, 60000);
  }

  test('without --json it is one line, not a stack trace', async () => {
    const locked = `${SCRATCH}/no-write-text`;
    mkdirSync(locked);
    chmodSync(locked, 0o555);
    try {
      const { stdout, stderr, exitCode } = await runCli([FIXTURE_PDF, '-o', `${locked}/x.epub`]);
      expect(exitCode).toBe(1);
      expect(stdout).toBe('');
      const lines = stderr.trim().split('\n');
      expect(lines).toHaveLength(1);
      expect(lines[0]).toStartWith('screepub: ');
      expect(lines[0]).toContain(`${locked}/x.epub`);
      expect(lines[0]).not.toContain('—');
    } finally {
      chmodSync(locked, 0o755);
    }
  }, 60000);

  test('a folder where the default output goes: named in plain words, no temp file left', async () => {
    // No -o, so no path to check up front: the book goes beside the input,
    // and a folder already sits at that name.
    const dir = `${SCRATCH}/default-collides`;
    mkdirSync(dir);
    writeFileSync(`${dir}/x.pdf`, new Uint8Array(await Bun.file(FIXTURE_PDF).arrayBuffer()));
    mkdirSync(`${dir}/x.epub`);
    const { stdout, exitCode } = await runCli([`${dir}/x.pdf`, '--no-fountain', '--json']);
    expect(exitCode).toBe(1);
    const answer = JSON.parse(stdout);
    expect(answer.error.code).toBe('unwritable');
    expect(answer.error.message).toContain(`${dir}/x.epub`);
    expect(answer.error.message).toContain('a folder, not a file');
    expect(answer.error.message).not.toContain('.tmp');
    expect(readdirSync(dir).sort()).toEqual(['x.epub', 'x.pdf']);
  }, 60000);

  test('--debug\'s dump, written in place rather than through a temp file, fails the same way', async () => {
    const dir = `${SCRATCH}/debug-collides`;
    mkdirSync(dir);
    mkdirSync(`${dir}/x.elements.json`);
    const { stdout, exitCode } = await runCli([
      FIXTURE_PDF, '-o', `${dir}/x.epub`, '--no-fountain', '--debug', '--json',
    ]);
    expect(exitCode).toBe(1);
    const answer = JSON.parse(stdout);
    expect(answer.error.code).toBe('unwritable');
    expect(answer.error.message).toContain(`${dir}/x.elements.json`);
    expect(answer.error.message).toContain('a folder, not a file');
  }, 60000);
});

describe('conversion error mapping', () => {
  test('password detection is typed, not substring', () => {
    expect(mapConversionError({ name: 'PasswordException', message: 'No password given' })?.code)
      .toBe('password');
    // A message that merely CONTAINS "password" (a file path, say) must
    // not classify — the old substring check did.
    expect(mapConversionError(new Error('/scripts/password-notes/x.pdf broke'))).toBeNull();
  });

  test('missing, directory, and permission errors are all unreadable', () => {
    for (const code of ['ENOENT', 'EISDIR', 'EACCES']) {
      const err = Object.assign(new Error(code), { code });
      expect(mapConversionError(err)?.code).toBe('unreadable');
    }
  });

  test('only the filesystem refusing counts as unwritable', () => {
    for (const code of ['ENOENT', 'EACCES', 'EPERM', 'EEXIST', 'ENOSPC', 'EROFS', 'EISDIR', 'ENOTDIR']) {
      expect(`${code}: ${isWriteRefusal(Object.assign(new Error(code), { code }))}`).toBe(`${code}: true`);
    }
    // A bug of ours must arrive as `internal`, not be dressed up as the
    // disk's fault.
    expect(isWriteRefusal(new TypeError('cannot read properties of undefined'))).toBe(false);
    expect(isWriteRefusal('a string throw')).toBe(false);
  });

  test('a corrupt PDF maps to unreadable with pdf.js named exceptions', () => {
    expect(mapConversionError({ name: 'InvalidPDFException', message: 'Invalid PDF structure.' })?.code)
      .toBe('unreadable');
  });
});

// --json's contract is that stdout is EXACTLY one parseable object: the app
// decodes it with JSONDecoder and a stray line would break every conversion.
// Progress therefore rides on stderr, and this is the test that keeps it
// there.
describe('cli --progress', () => {
  test('keeps stdout a single JSON object while reporting on stderr', async () => {
    const { stdout, stderr, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', `${SCRATCH}/prog.epub`, '--no-fountain',
      '--json', '--progress',
    ]);
    expect(exitCode).toBe(0);

    // The whole of stdout, parsed as one value. Not "the first line".
    const result = JSON.parse(stdout);
    expect(result.ok).toBe(true);

    // Every stderr line is a progress event. The one 'page' event carries the
    // first real page's text (tests/first-page.test.ts); the rest are ticks.
    const events = stderr.trim().split('\n').filter(Boolean).map((l) => JSON.parse(l).progress);
    expect(events.filter((e) => e.stage === 'page').length).toBe(1);
    const ticks = events.filter((e) => e.stage !== 'page');
    expect(ticks.length).toBeGreaterThan(0);
    for (const t of ticks) {
      expect(['parse', 'render']).toContain(t.stage);
      expect(t.percent).toBeGreaterThanOrEqual(0);
      expect(t.percent).toBeLessThanOrEqual(100);
    }

    // A reading tick names the page it has reached and how many there are,
    // for the window's "Reading page N of M" (spec decision 17).
    const reading = ticks.filter((t) => t.stage === 'parse');
    expect(reading.length).toBeGreaterThan(0);
    for (const t of reading) {
      expect(Number.isInteger(t.page)).toBe(true);
      expect(t.page).toBeGreaterThanOrEqual(1);
      expect(t.page).toBeLessThanOrEqual(t.pages);
    }
    expect(reading.at(-1).page).toBe(reading.at(-1).pages);

    // A bar that goes backwards is worse than no bar.
    const percents = ticks.map((t: { percent: number }) => t.percent);
    expect([...percents].sort((a, b) => a - b)).toEqual(percents);
    expect(percents.at(-1)).toBe(100);

    // The book starts being built while the bar still says 85: that first
    // render tick must be said, or the window keeps saying "Reading page M
    // of M" until the book is done.
    const firstRender = ticks.findIndex((t) => t.stage === 'render');
    expect(ticks[firstRender].percent).toBeLessThan(100);
  }, 60000);

  test('is opt-in: no --progress means no stderr', async () => {
    const { stderr, exitCode } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', `${SCRATCH}/noprog.epub`, '--no-fountain', '--json',
    ]);
    expect(exitCode).toBe(0);
    expect(stderr).toBe('');
  }, 60000);
});

describe('the default conversion path is unchanged by verb dispatch', () => {
  test('a PDF whose stem is a verb converts exactly as any other would', async () => {
    // Names that brush against dispatch: "send.pdf" starts with a verb, and
    // "devices.pdf" is the shadowing rule's near miss. Both must take the
    // ordinary path and produce the ordinary success payload.
    for (const name of ['send.pdf', 'devices.pdf']) {
      const input = `${SCRATCH}/${name}`;
      writeFileSync(input, new Uint8Array(await Bun.file(`${FIXTURES}screenplay.pdf`).arrayBuffer()));
      const out = `${SCRATCH}/${name}.epub`;
      const { stdout, exitCode } = await runCli([input, '-o', out, '--no-fountain', '--json']);
      expect(exitCode).toBe(0);
      const result = JSON.parse(stdout);
      expect(result.ok).toBe(true);
      expect(result.epubPath).toBe(out);
      expect(result.pages).toBeGreaterThan(0);
    }
  }, 120000);
});

describe('--options-json', () => {
  test('applies a knob passed as an argv string', async () => {
    const previewPath = `${SCRATCH}/options-json-applies.html`;
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-html', previewPath,
      '-o', `${SCRATCH}/options-json-applies.epub`, '--no-fountain',
      '--options-json', '{"dialogueSideMarginPct":7}']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(true);
    const html = await Bun.file(previewPath).text();
    // Not "it didn't crash": the number we passed has to reach the CSS.
    // The brief's own version of this assertion (bare "7%" / not "20%")
    // is a false negative waiting to happen: section.titlepage carries a
    // static `margin-top: 20%` unrelated to this knob, so "not contain
    // 20%" would fail against a CORRECT implementation the day someone
    // reads that titlepage rule. Anchor on the actual declaration.
    expect(html).toContain('margin-left: 7%');
    expect(html).not.toContain('margin-left: 20%'); // the default this overrode
  }, 60000);

  test('clamps out-of-range values instead of trusting them', async () => {
    const previewPath = `${SCRATCH}/options-json-clamps.html`;
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-html', previewPath,
      '-o', `${SCRATCH}/options-json-clamps.epub`, '--no-fountain',
      '--options-json', '{"dialogueSideMarginPct":999}']);
    expect(JSON.parse(out.stdout).ok).toBe(true);
    const html = await Bun.file(previewPath).text();
    expect(html).toContain('margin-left: 30%'); // resolveFormatOptions' documented ceiling
    expect(html).not.toContain('margin-left: 999%');
  }, 60000);

  test('rejects a non-object payload with bad-options', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--options-json', '[1,2]']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(false);
    expect(answer.error.code).toBe('bad-options');
    expect(answer.error.message).toContain('--options-json');
  });

  test('rejects malformed JSON with bad-options and does not leak the payload', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--options-json', '{oops']);
    const answer = JSON.parse(out.stdout);
    expect(answer.error.code).toBe('bad-options');
    expect(answer.error.message).not.toContain('oops');
  });

  test('refuses both --options and --options-json rather than picking one', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--options', 'x.json',
      '--options-json', '{}']);
    const answer = JSON.parse(out.stdout);
    expect(answer.error.code).toBe('bad-options');
    expect(answer.error.message).toContain('not both');
  });

  // Beyond the brief: resolveFormatOptions ignores unknown keys and falls
  // back to the default for a wrong-typed value rather than rejecting the
  // whole payload — that's the SAME merge path --options already uses, so
  // --options-json must inherit that behavior rather than validating twice
  // (a second, stricter check here would be the "two merge rules" the task
  // exists to avoid). These tests catch an implementation that skips
  // resolveFormatOptions and spreads the parsed JSON directly: that would
  // either crash rendering or leak the raw string into the CSS.
  test('ignores an unknown key without failing the conversion', async () => {
    const previewPath = `${SCRATCH}/options-json-unknown-key.html`;
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-html', previewPath,
      '-o', `${SCRATCH}/options-json-unknown-key.epub`, '--no-fountain',
      '--options-json', '{"notARealKnob":123,"dialogueSideMarginPct":12}']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(true);
    const html = await Bun.file(previewPath).text();
    // The unknown key is dropped silently; the valid sibling key still lands.
    expect(html).toContain('margin-left: 12%');
  }, 60000);

  test('falls back to the default for a wrong-typed value instead of crashing', async () => {
    const previewPath = `${SCRATCH}/options-json-wrong-type.html`;
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-html', previewPath,
      '-o', `${SCRATCH}/options-json-wrong-type.epub`, '--no-fountain',
      '--options-json', '{"dialogueSideMarginPct":"wide"}']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(true);
    const html = await Bun.file(previewPath).text();
    // A naive `JSON.parse` + spread would template the raw string straight
    // into the CSS ("wide%"); resolveFormatOptions must fall back to the
    // documented default instead.
    expect(html).not.toContain('wide%');
    expect(html).toContain('margin-left: 20%');
  }, 60000);
});

describe('--preview-inline', () => {
  test('puts the same document in the JSON that --preview-html writes to disk', async () => {
    const previewPath = `${SCRATCH}/preview-inline-parity.html`;
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-inline',
      '--preview-html', previewPath, '-o', `${SCRATCH}/preview-inline-parity.epub`,
      '--no-fountain']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(true);
    const onDisk = await Bun.file(previewPath).text();
    // Byte equality, not "contains something": the reader's whole premise is
    // that what you proof is what ships, so two producers would be a defect.
    expect(answer.previewHtml).toBe(onDisk);
  }, 60000);

  test('the inlined document carries the stylesheet, not a link to one', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-inline',
      '-o', `${SCRATCH}/preview-inline-style.epub`, '--no-fountain']);
    const { previewHtml } = JSON.parse(out.stdout);
    expect(previewHtml).toContain('<style>');
    expect(previewHtml).not.toContain('<link rel="stylesheet"');
    expect(previewHtml).toContain('h2.scene-heading');
  }, 60000);

  test('stdout is still exactly one JSON object', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-inline',
      '-o', `${SCRATCH}/preview-inline-single-line.epub`, '--no-fountain']);
    expect(out.stdout.trim().split('\n')).toHaveLength(1);
    expect(() => JSON.parse(out.stdout)).not.toThrow();
  }, 60000);

  test('the key is absent unless asked for', async () => {
    const out = await runCli([FIXTURE_PDF, '--json',
      '-o', `${SCRATCH}/preview-inline-absent.epub`, '--no-fountain']);
    expect(JSON.parse(out.stdout).previewHtml).toBeUndefined();
  }, 60000);

  test('it is a usage error without --json', async () => {
    const out = await runCli([FIXTURE_PDF, '--preview-inline']);
    expect(out.exitCode).toBe(1);
    expect(out.stderr).toContain('--preview-inline');
  }, 60000);

  // Beyond the brief: the absent-key test above proves the flag stays off
  // by default, but not that turning it on actually does something beyond
  // "doesn't crash" when no --preview-html is also given (that's the ONLY
  // combination the app itself will ever use — the window has no
  // filesystem to point --preview-html at). Assert the content, not just
  // its presence, so a stub that emits `previewHtml: ''` would fail here.
  test('with no --preview-html, the inlined document is still the real preview', async () => {
    const out = await runCli([FIXTURE_PDF, '--json', '--preview-inline',
      '-o', `${SCRATCH}/preview-inline-standalone.epub`, '--no-fountain']);
    const answer = JSON.parse(out.stdout);
    expect(answer.ok).toBe(true);
    expect(typeof answer.previewHtml).toBe('string');
    expect(answer.previewHtml.length).toBeGreaterThan(1000);
    // The preview document has no title page (that's an EPUB-only section),
    // so anchor on body content the fixture is known to render instead.
    expect(answer.previewHtml).toContain('<p class="character">MARGO</p>');
    expect(answer.previewHtmlPath).toBeUndefined();
  }, 60000);
});

describe('a large answer survives a pipe', () => {
  // The desktop window reads the engine's stdout through a pipe, and a pipe
  // buffer is 64 KiB. `console.log` to a pipe is buffered, and exiting does
  // not wait for the tail: measured before the fix, the answer for the
  // script generated below reached a piped caller cut to exact 64 KiB
  // multiples (262,144 / 589,824 / 655,360 …) with no app and no Tauri
  // anywhere, while the same command redirected to a FILE was always whole.
  // The engine's own suite never saw it because every other answer here fits
  // in one buffer; --preview-inline is 1.85-2.6 KB per page, so a 120-page
  // script is already several buffers deep.
  //
  // HOW THIS TEST IS SHAPED, because the obvious shapes do not work and one
  // of them shipped here and had to be replaced.
  //
  // The loss is a race between the process exiting and its last writes
  // leaving. So the reader must be FAST, not slow. A deliberately slow
  // reader — the first version of this test — makes the race LESS likely,
  // not more: backpressure keeps the child alive until it has flushed, which
  // is the thing being tested for. Measured per attempt with the defect
  // restored, 20 attempts each: reading at full speed lost 30%, pausing 5 ms
  // between reads lost 10%, pausing 50 ms lost 0%, and waiting 400 ms before
  // reading at all — what this test used to do — lost 5%. Six attempts at 5%
  // is a test that passes against a live defect most of the time, and a
  // reviewer duly caught it doing so (11 of 16 runs).
  //
  // What does work is CONTENDING for the machine: the attempts run in
  // PARALLEL, so eight conversions race each other and the scheduler. That
  // raises the loss rate and cuts the wall clock at the same time — eight
  // attempts take about half a second in total, where six serial ones took
  // three seconds.
  //
  // Measured with the defect restored: 20 rounds of 8, every round caught it,
  // and the WORST round still lost 3 of its 8 attempts (best, 7 of 8). So
  // this is not a knife edge. With the fix in place, 20 rounds of 8 — 160
  // attempts — lost nothing, so there are no false reds either.
  const ATTEMPTS = 8;

  /** An invented screenplay, big enough to need several pipe buffers. */
  function generated(scenes: number) {
    const parts = ['Title: Generated Load Sample\nAuthor: Test Harness\n\n'];
    for (let i = 0; i < scenes; i += 1) {
      parts.push(`INT. TEST ROOM ${i} - DAY\n\n`);
      parts.push(`A plain room with a numbered door. Nothing in it matters except its
length, which is the whole point of sample number ${i}.\n\n`);
      parts.push(`ALPHA\nThis is line ${i} of a script that exists only to be big.\n\n`);
      parts.push(`BETA\n(flatly)\nUnderstood. That was line ${i}. There will be more.\n\n`);
    }
    return parts.join('');
  }

  test('every byte of it, every time', async () => {
    const script = `${SCRATCH}/pipe-load.fountain`;
    writeFileSync(script, generated(1000), 'utf8');

    // One output path each: the attempts run at once and must not race for a
    // file as well as for the CPU.
    const answers = await Promise.all(
      Array.from({ length: ATTEMPTS }, (_, i) =>
        runCli([script, '--json', '--preview-inline',
          '--no-fountain', '-o', `${SCRATCH}/pipe-load-${i}.epub`])),
    );

    const lengths: number[] = [];
    for (const [i, out] of answers.entries()) {
      const stdout = out.stdout.trim();
      lengths.push(stdout.length);
      // Named by attempt, so a failure says which run lost its tail.
      expect(`attempt ${i} parses: ${(() => {
        try {
          return JSON.parse(stdout).ok === true;
        } catch {
          return false;
        }
      })()}`).toBe(`attempt ${i} parses: true`);
    }

    // The answer is the same every time, so any difference is loss. (A
    // truncation that happened to cut identically on every attempt would
    // still be caught by the parse above.)
    expect(lengths.map((l) => l === lengths[0])).toEqual(lengths.map(() => true));
    // And the test is actually testing the thing: an answer that fits in one
    // 64 KiB pipe buffer could never have shown the defect.
    expect(lengths[0]).toBeGreaterThan(4 * 65536);
  }, 120000);
});
