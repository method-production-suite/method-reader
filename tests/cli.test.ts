import { afterAll, describe, test, expect } from 'bun:test';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import JSZip from 'jszip';
import { mapConversionError, unwritableError } from '../src/cli-errors';
import { encodePng } from '../src/parser/png';

const ROOT = new URL('..', import.meta.url).pathname;
const FIXTURES = new URL('./fixtures/', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-'));
const FIXTURE_PDF = `${FIXTURES}screenplay.pdf`;

afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

async function runCli(args: string[], env: Record<string, string> = {}) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
    env: { ...process.env, ...env },
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
    // Every optional output the sample carries is requested, so each of its
    // keys is present. Three keys are conditional and absent from the sample
    // on purpose: imagesPath and imageCount appear only when the conversion
    // carried pictures (screenplay.pdf has none; '--json reports the folder
    // and the count' below pins both on images.pdf), and settingsPath only
    // when the conversion used the script's saved settings (a
    // <script>.screepub.json sidecar). The sample is the SAME file kit-check
    // decodes into EngineResult, so a key rename must break one suite or the
    // other before it breaks the app.
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

// The CLI owns disk, so this is where "a conversion writes pictures" is a
// fact rather than a plan. Every test gets its own directory: the whole
// subject is what else ends up in the folder. The folder's own policy (what
// may be read, what may be deleted) is tested directly in
// library-images.test.ts; these are about the WIRING.
describe('the picture folder beside the .fountain', () => {
  let seq = 0;
  const freshDir = () => {
    const dir = join(SCRATCH, `images-${seq++}`);
    mkdirSync(dir);
    return dir;
  };
  const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  /** A REAL one-pixel picture, told from another by that pixel's colour. */
  const png = (tag = 0) => encodePng(new Uint8Array([tag, 0, 0]), 1, 1);

  test('a PDF with pictures writes them beside its .fountain', async () => {
    const dir = freshDir();
    const { exitCode } = await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    expect(exitCode).toBe(0);
    expect(existsSync(join(dir, 'out.fountain'))).toBe(true);
    // Exactly the folder's contents, so a leftover `.p001-01.png.<pid>.tmp`
    // from the atomic writer would fail here rather than sit in the library.
    expect(readdirSync(join(dir, 'out.images')).sort()).toEqual([
      'p001-01.png',
      'p002-01.png',
    ]);
    const first = readFileSync(join(dir, 'out.images', 'p001-01.png'));
    expect([...first.subarray(0, 8)]).toEqual(PNG_MAGIC);
    expect(first.length).toBeGreaterThan(100);
  }, 60000);

  test('every picture written is one the .fountain asks for', async () => {
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    const text = readFileSync(join(dir, 'out.fountain'), 'utf8');
    for (const name of readdirSync(join(dir, 'out.images'))) {
      expect(text).toContain(`[[img: ${name}`);
    }
  }, 60000);

  test('--json reports the folder and the count', async () => {
    const dir = freshDir();
    const { stdout } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    const res = JSON.parse(stdout);
    expect(res.ok).toBe(true);
    expect(res.imagesPath).toBe(join(dir, 'out.images'));
    expect(res.imageCount).toBe(2);
    expect(res.warnings).toEqual([]);
  }, 60000);

  test('the human output names the folder it wrote', async () => {
    const dir = freshDir();
    const { stdout } = await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    expect(stdout).toContain(`wrote ${join(dir, 'out.images')}`);
  }, 60000);

  test('a script with no pictures gets no folder and no fields', async () => {
    const dir = freshDir();
    const { stdout } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    const res = JSON.parse(stdout);
    expect(res.imagesPath).toBeUndefined();
    expect(res.imageCount).toBeUndefined();
    expect(existsSync(join(dir, 'out.images'))).toBe(false);
  }, 60000);

  test('under --library the folder sits in the script\'s own library folder', async () => {
    // One rule, beside the .fountain, and in the library the .fountain lives
    // in the script's folder, so that is where its pictures go too.
    const dir = freshDir();
    const scripts = join(dir, 'scripts');
    const root = join(dir, 'lib');
    mkdirSync(scripts);
    copyFileSync(`${FIXTURES}images.pdf`, join(scripts, 'Plates.pdf'));
    const { stdout, exitCode } = await runCli(
      [join(scripts, 'Plates.pdf'), '--library', '--json'], { SCREEPUB_LIBRARY: root },
    );
    expect(exitCode).toBe(0);
    const res = JSON.parse(stdout);
    expect(res.fountainPath).toBe(join(root, 'Plates', 'Plates.fountain'));
    expect(res.imagesPath).toBe(join(root, 'Plates', 'Plates.images'));
    expect(readdirSync(res.imagesPath).sort()).toEqual(['p001-01.png', 'p002-01.png']);
    // And nothing beside the PDF: the library conversion writes into the
    // library only.
    expect(readdirSync(scripts)).toEqual(['Plates.pdf']);
  }, 60000);

  test('a re-conversion that lost its art clears the folder it left', async () => {
    // The ordinary case of a writer cutting the pictures from a draft: the
    // whole folder is stale, and leaving it would keep another draft's art in
    // the library under this book's name, with no imagesPath in the JSON to
    // hint at it.
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    expect(readdirSync(join(dir, 'out.images')).length).toBe(2);

    const { stdout } = await runCli([
      `${FIXTURES}screenplay.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    expect(JSON.parse(stdout).imageCount).toBeUndefined();
    expect(existsSync(join(dir, 'out.images'))).toBe(false);
  }, 60000);

  test('a folder that cannot be written is unwritable, and says the book is there', async () => {
    // Partial output is the contract: the EPUB was written before the folder
    // failed, and a book the reader can open is not thrown away because its
    // pictures could not be saved. So the answer is ok:false and the message
    // says what is on disk. It names the folder the user would look for,
    // never a temp file.
    const dir = freshDir();
    writeFileSync(join(dir, 'out.images'), 'a file, not a folder');
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    expect(exitCode).toBe(1);
    const err = JSON.parse(stdout).error;
    expect(err.code).toBe('unwritable');
    expect(err.message).toContain(join(dir, 'out.images'));
    expect(err.message).toContain(`${join(dir, 'out.epub')} was already written`);
    expect(err.message).not.toContain('.tmp');
    expect(existsSync(join(dir, 'out.epub'))).toBe(true);
    // Pictures before text: a failed folder stops the run before the
    // .fountain is written, so a first conversion leaves none and a
    // re-conversion leaves the OLD one, still naming the old pictures (the
    // next test). Either way the message says it was not updated.
    expect(existsSync(join(dir, 'out.fountain'))).toBe(false);
    expect(err.message).toContain('The .fountain was not updated.');
  }, 60000);

  test('a failed RE-conversion keeps the old .fountain, naming the old pictures', async () => {
    // The same failure one conversion later, which is the common one: the
    // .fountain from the run before is still there, still names that run's
    // pictures, and the new EPUB beside it does not match it. Nothing here
    // deletes it (a .fountain the user can re-render is not ours to throw
    // away), so the message has to say it was not updated.
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    // Marked, because the second run converts the SAME PDF: a .fountain
    // rewritten ahead of its pictures would come out byte-identical to the
    // old one, and an unmarked comparison would pass with the order broken.
    writeFileSync(join(dir, 'out.fountain'), '\n[[marker: from the run before]]\n', { flag: 'a' });
    const before = readFileSync(join(dir, 'out.fountain'), 'utf8');
    expect(before).toContain('[[img: p001-01.png');
    chmodSync(join(dir, 'out.images'), 0o555);
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    chmodSync(join(dir, 'out.images'), 0o755);
    expect(exitCode).toBe(1);
    const err = JSON.parse(stdout).error;
    expect(err.code).toBe('unwritable');
    expect(err.message).toContain('The .fountain was not updated.');
    expect(readFileSync(join(dir, 'out.fountain'), 'utf8')).toBe(before);
    expect(before).toContain('[[marker: from the run before]]');
  }, 60000);

  test('a picture folder that is a symlink is unwritable, and its target is untouched', async () => {
    // Adopted, the link was written THROUGH and its target pruned: the
    // target gained the new pictures and lost a p009-01.png of its own.
    const dir = freshDir();
    const elsewhere = freshDir();
    writeFileSync(join(elsewhere, 'p009-01.png'), png(9));
    symlinkSync(elsewhere, join(dir, 'out.images'));
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    expect(exitCode).toBe(1);
    const err = JSON.parse(stdout).error;
    expect(err.code).toBe('unwritable');
    expect(err.message).toContain(join(dir, 'out.images'));
    expect(readdirSync(elsewhere)).toEqual(['p009-01.png']);
    expect(existsSync(join(dir, 'out.fountain'))).toBe(false);
  }, 60000);

  test('a script with no pictures never prunes through a symlinked folder', async () => {
    // The other door: with nothing to write the CLI only prunes, and the
    // prune would delete the link target's p009-01.png as a stale picture.
    const dir = freshDir();
    const elsewhere = freshDir();
    writeFileSync(join(elsewhere, 'p009-01.png'), png(9));
    symlinkSync(elsewhere, join(dir, 'out.images'));
    const { exitCode } = await runCli([`${FIXTURE_PDF}`, '-o', join(dir, 'out.epub')]);
    expect(exitCode).toBe(0);
    expect(readdirSync(elsewhere)).toEqual(['p009-01.png']);
  }, 60000);

  test('a folder whose parent is missing is unwritable, and no parent is made', async () => {
    // --fountain into a directory that does not exist. The folder is made,
    // never its parents: building them would put the pictures somewhere the
    // user did not ask for.
    const dir = freshDir();
    const fountain = join(dir, 'missing', 'deeper', 'x.fountain');
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--fountain', fountain, '--json',
    ]);
    expect(exitCode).toBe(1);
    const err = JSON.parse(stdout).error;
    expect(err.code).toBe('unwritable');
    expect(err.message).toContain(join(dir, 'missing', 'deeper', 'x.images'));
    expect(err.message).toContain('(ENOENT)');
    expect(err.message).not.toContain('.tmp');
    expect(readdirSync(dir)).toEqual(['out.epub']);
  }, 60000);

  // The scanned guard throws inside convert.ts, before it returns anything,
  // which is the only reason full-page PNGs do not land on someone's disk for
  // a conversion that failed. (blank-pages.pdf carries no images of its own;
  // what it pins is that the failing path writes NOTHING, folder included.)
  test('a scanned PDF writes no folder at all', async () => {
    const dir = freshDir();
    const { stdout, exitCode } = await runCli([
      `${FIXTURES}blank-pages.pdf`, '-o', join(dir, 'out.epub'), '--json',
    ]);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error.code).toBe('scanned');
    expect(readdirSync(dir)).toEqual([]);
  }, 60000);

  test('--no-fountain writes no folder either', async () => {
    // The folder is the .fountain's other half: the notes are what name these
    // files, so suppressing one suppresses both. Today that does discard the
    // pixels (the book shows no pictures yet, so it holds no copy of its own),
    // which is what asking for no intermediates means.
    const dir = freshDir();
    const { stdout } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'), '--no-fountain', '--json',
    ]);
    const res = JSON.parse(stdout);
    expect(res.imagesPath).toBeUndefined();
    // Still reported as CARRIED: a fact about the conversion, not about any
    // folder, and the conversion produced them.
    expect(res.imageCount).toBe(2);
    expect(readdirSync(dir)).toEqual(['out.epub']);
  }, 60000);

  test('--fountain elsewhere takes the folder with it', async () => {
    // The notes live in the .fountain, so the pictures follow the .fountain
    // and not the EPUB. Beside the EPUB they would be a folder nothing
    // references, and a render from the text would find nothing.
    const dir = freshDir();
    const other = join(dir, 'text');
    mkdirSync(other);
    const { stdout } = await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'),
      '--fountain', join(other, 'script.fountain'), '--json',
    ]);
    expect(JSON.parse(stdout).imagesPath).toBe(join(other, 'script.images'));
    expect(existsSync(join(dir, 'out.images'))).toBe(false);
    expect(readdirSync(join(other, 'script.images')).length).toBe(2);
  }, 60000);

  // THE data-loss test. Converting into a directory that already holds
  // someone's own pictures must not delete them: the read side's allowlist
  // accepts any name a note could carry, so `board-01.png` and `sunset.png`
  // would both look like ours to a prune written against it. Only names
  // `imageFileName` can produce may ever be removed.
  test('pictures the user put in the folder are never deleted', async () => {
    const dir = freshDir();
    const folder = join(dir, 'lookbook.images');
    mkdirSync(folder);
    for (const name of ['board-01.png', 'sunset.png', 'A-01.png', 'p1.png', 'readme.md']) {
      writeFileSync(join(folder, name), png());
    }
    await runCli([
      `${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub'),
      '--fountain', join(dir, 'lookbook.fountain'),
    ]);
    expect(readdirSync(folder).sort()).toEqual([
      'A-01.png',
      'board-01.png',
      'p001-01.png',
      'p002-01.png',
      'p1.png',
      'readme.md',
      'sunset.png',
    ]);
  }, 60000);

  test('a crashed run\'s temp file does not live in the folder forever', async () => {
    // The atomic writer writes `.name.<pid>.tmp` then renames; a crash in
    // between leaves one, and nothing else would ever remove it (it is not a
    // `.png`, so a reader never even sees it).
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    writeFileSync(join(dir, 'out.images', '.p001-01.png.999.tmp'), png());
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    expect(readdirSync(join(dir, 'out.images')).sort()).toEqual([
      'p001-01.png',
      'p002-01.png',
    ]);
  }, 60000);

  test('a second conversion clears the first run\'s stale pictures', async () => {
    // Names are per page, so an edit that moves a picture leaves the old file
    // behind, and whatever packages a book from this folder would package it.
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    writeFileSync(join(dir, 'out.images', 'p009-09.png'), png());
    writeFileSync(join(dir, 'out.images', 'notes.txt'), 'mine');
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    // The prune is bounded to our own pictures: anything else is not ours.
    expect(readdirSync(join(dir, 'out.images')).sort()).toEqual([
      'notes.txt',
      'p001-01.png',
      'p002-01.png',
    ]);
  }, 60000);

  test('a picture is written whole or not at all', async () => {
    // Temp then rename, and the temp name (`.name.pid.tmp`) is not one a
    // reader picks up, so a reader racing a conversion sees whole files or no
    // file, never half of one.
    const dir = freshDir();
    await runCli([`${FIXTURES}images.pdf`, '-o', join(dir, 'out.epub')]);
    for (const name of readdirSync(join(dir, 'out.images'))) {
      expect(name.startsWith('.')).toBe(false);
      expect(statSync(join(dir, 'out.images', name)).size).toBeGreaterThan(100);
    }
  }, 60000);
});

// The other direction: a `.fountain` handed back to the CLI (the window's
// re-render does exactly this) finds its pictures in the same folder a PDF
// conversion wrote. What the reader accepts is library-images.test.ts's
// subject; these are about the wiring, and about the folder being READ here
// and never written.
describe('a .fountain input reads its picture folder', () => {
  let seq = 0;
  /** A script whose notes name two pictures, and the folder beside it. */
  const script = () => {
    const dir = join(SCRATCH, `fountain-images-${seq++}`);
    mkdirSync(dir);
    writeFileSync(
      join(dir, 'x.fountain'),
      'Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\n'
        + 'Between.\n\n[[img: p002-01.png w=60]]\n\nAfter.\n',
    );
    mkdirSync(join(dir, 'x.images'));
    return dir;
  };
  const png = (tag = 0) => encodePng(new Uint8Array([tag, 0, 0]), 1, 1);
  const convert = async (dir: string, ...extra: string[]) => {
    const { stdout, exitCode } = await runCli([
      join(dir, 'x.fountain'), '-o', join(dir, 'x.epub'), '--json', ...extra,
    ]);
    expect(exitCode).toBe(0);
    const res = JSON.parse(stdout);
    const zip = await JSZip.loadAsync(readFileSync(join(dir, 'x.epub')));
    const body = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    const images = Object.keys(zip.files).filter((n) => n.startsWith('OEBPS/images/') && !zip.files[n].dir).sort();
    return { res, body, images };
  };
  const figure = (name: string, w: number) =>
    `<p class="figure w${w}"><img src="../images/${name}" alt="Illustration"/></p>`;

  test('the notes render the pictures beside them, and the book carries them', async () => {
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), png(1));
    writeFileSync(join(dir, 'x.images', 'p002-01.png'), png(2));
    const { res, body, images } = await convert(dir);
    expect(res.warnings).toEqual([]);
    expect(body).toContain(figure('p001-01.png', 40));
    expect(body).toContain(figure('p002-01.png', 60));
    expect(images).toEqual(['OEBPS/images/p001-01.png', 'OEBPS/images/p002-01.png']);
    // Nothing was written, so the answer names no folder and counts nothing:
    // both fields are about what a PDF conversion carried out.
    expect(res.imagesPath).toBeUndefined();
    expect(res.imageCount).toBeUndefined();
  }, 60000);

  test('the window’s re-render answer shows the same pictures, each one inside its figure', async () => {
    // Exactly the window's Tune call: the .fountain, --json, --preview-inline.
    // The preview has no folder to point into, so each picture rides in its
    // `<img>` as a data: URI, in the figure the book uses.
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), png(1));
    writeFileSync(join(dir, 'x.images', 'p002-01.png'), png(2));
    const { res } = await convert(dir, '--preview-inline');
    const inlined = (tag: number, w: number) =>
      `<p class="figure w${w}"><img src="data:image/png;base64,${Buffer.from(png(tag)).toString('base64')}" alt="Illustration"/></p>`;
    expect(res.previewHtml).toContain(inlined(1, 40));
    expect(res.previewHtml).toContain(inlined(2, 60));
  }, 60000);

  test('a picture the folder will not give up is a warning, and no figure', async () => {
    // One file that is not a PNG under a PNG's name, and one symlink to a
    // perfectly good picture: the reader refuses both, and each is a plate
    // missing from the book that someone has to be told about.
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), 'not a picture');
    writeFileSync(join(dir, 'elsewhere.png'), png(2));
    symlinkSync(join(dir, 'elsewhere.png'), join(dir, 'x.images', 'p002-01.png'));
    const { res, body, images } = await convert(dir);
    expect(res.warnings).toEqual([
      "2 pictures could not be read out of this script's picture folder, so they were left out.",
    ]);
    expect(body).not.toContain('class="figure');
    expect(images).toEqual([]);
  }, 60000);

  test('with includeImages off, the folder is not read at all', async () => {
    // A folder that warns whenever it is read, so silence is proof it was
    // not: off means no figure, and no sentence about pictures the reader
    // chose not to see.
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), 'not a picture');
    writeFileSync(join(dir, 'x.images', 'p002-01.png'), png(2));
    const { res, body, images } = await convert(dir, '--options-json', '{"includeImages":false}');
    expect(res.warnings).toEqual([]);
    expect(body).not.toContain('class="figure');
    expect(images).toEqual([]);
    // The control: the same folder, read, does warn.
    const on = await convert(dir);
    expect(on.res.warnings).toEqual([
      "1 picture could not be read out of this script's picture folder, so it was left out.",
    ]);
  }, 60000);

  test('the folder is never written or pruned, and none is made', async () => {
    // A PDF conversion owns the folder; a render from the text only reads
    // it. A stale picture of our own naming and a crashed run's temp would
    // both go under a PDF conversion's prune, and both stay here.
    const dir = script();
    const kept = ['.p003-01.png.999.tmp', 'p001-01.png', 'p002-01.png', 'p009-09.png'];
    for (const name of kept) writeFileSync(join(dir, 'x.images', name), png());
    await convert(dir);
    expect(readdirSync(join(dir, 'x.images')).sort()).toEqual(kept);
    const bare = script();
    rmSync(join(bare, 'x.images'), { recursive: true });
    const { body } = await convert(bare);
    expect(body).not.toContain('class="figure');
    expect(existsSync(join(bare, 'x.images'))).toBe(false);
  }, 60000);

  // The window's Tune re-renders the library book this way on every knob
  // move, so a picture the script shows and the read cannot supply is a
  // figure gone from the book on disk. The book is still written (Tune has
  // to keep working), and the answer says what it lacks.
  test('a picture the folder does not hold is a warning, and the book is still made', async () => {
    // An iCloud copy evicted while offline is renamed `.p002-01.png.icloud`,
    // so the folder no longer lists the picture under any name a note uses.
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), png(1));
    writeFileSync(join(dir, 'x.images', '.p002-01.png.icloud'), '');
    const { res, body, images } = await convert(dir);
    expect(res.ok).toBe(true);
    expect(res.warnings).toEqual([
      '1 picture this script shows could not be found in its picture folder, so the book was made without it.',
    ]);
    expect(body).toContain(figure('p001-01.png', 40));
    expect(images).toEqual(['OEBPS/images/p001-01.png']);
  }, 60000);

  test('a folder that is not there loses every picture, said once', async () => {
    const dir = script();
    rmSync(join(dir, 'x.images'), { recursive: true });
    const { res, body } = await convert(dir);
    expect(res.warnings).toEqual([
      '2 pictures this script shows could not be found in its picture folder, so the book was made without them.',
    ]);
    expect(body).not.toContain('class="figure');
  }, 60000);

  test('each lost picture is said once, under what happened to it', async () => {
    // p001 is listed and refused (not a PNG), p002 is not listed at all. The
    // refused one is also absent from what the read hands over, and counting
    // it again as not found would report three pictures lost out of two.
    const dir = script();
    writeFileSync(join(dir, 'x.images', 'p001-01.png'), 'not a picture');
    const { res } = await convert(dir);
    expect(res.warnings).toEqual([
      "1 picture could not be read out of this script's picture folder, so it was left out.",
      '1 picture this script shows could not be found in its picture folder, so the book was made without it.',
    ]);
  }, 60000);

  test('a folder that is a link is said once, as a link', async () => {
    const dir = script();
    rmSync(join(dir, 'x.images'), { recursive: true });
    mkdirSync(join(dir, 'elsewhere'));
    writeFileSync(join(dir, 'elsewhere', 'p001-01.png'), png(1));
    symlinkSync(join(dir, 'elsewhere'), join(dir, 'x.images'));
    const { res } = await convert(dir);
    expect(res.warnings).toEqual([
      "This script's picture folder is not a folder of its own (it is a link or a file), so no pictures were read from it.",
    ]);
  }, 60000);

  test('with includeImages off, a missing picture is not a loss and says nothing', async () => {
    const dir = script();
    rmSync(join(dir, 'x.images'), { recursive: true });
    const { res } = await convert(dir, '--options-json', '{"includeImages":false}');
    expect(res.warnings).toEqual([]);
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

  test('a corrupt PDF maps to unreadable with pdf.js named exceptions', () => {
    expect(mapConversionError({ name: 'InvalidPDFException', message: 'Invalid PDF structure.' })?.code)
      .toBe('unreadable');
  });
});

describe('a picture folder that cannot be written', () => {
  const refusal = (code: string) =>
    Object.assign(new Error(`${code}: open '/lib/.p001-01.png.80708.tmp'`), { code });

  test('is unwritable for every way a disk says no, naming the folder', () => {
    for (const code of ['ENOENT', 'EACCES', 'EPERM', 'EEXIST', 'ENOSPC', 'EROFS', 'EISDIR', 'ENOTDIR']) {
      const err = unwritableError('/lib/A.images', refusal(code), []);
      expect(err?.code).toBe('unwritable');
      expect(err?.message).toContain('/lib/A.images');
      expect(err?.message).toContain(code);
      // The temp path the error came from is an implementation detail.
      expect(err?.message).not.toContain('.tmp');
    }
  });

  test('says what was already written, in one sentence that agrees with itself', () => {
    expect(unwritableError('/lib/A.images', refusal('ENOSPC'), ['/lib/A.epub'])?.message)
      .toBe(
        'cannot write the pictures to /lib/A.images (ENOSPC). /lib/A.epub was already written. ' +
          'The .fountain was not updated.',
      );
    expect(unwritableError('/lib/A.images', refusal('ENOSPC'), ['/lib/A.epub', '/lib/A.mobi'])?.message)
      .toContain('/lib/A.epub, /lib/A.mobi were already written.');
  });

  test('says the .fountain was not updated, which is always true here', () => {
    // The folder is written before the .fountain, so on this path the
    // .fountain is never the new one: a first conversion has none, and a
    // re-conversion keeps the previous one, naming the previous pictures.
    expect(unwritableError('/lib/A.images', refusal('EACCES'), [])?.message)
      .toBe('cannot write the pictures to /lib/A.images (EACCES). The .fountain was not updated.');
  });

  test('anything that is not the disk refusing is left to the catch-all', () => {
    // A bug of ours must still arrive as `internal`, not dressed up as the
    // disk's fault.
    expect(unwritableError('/lib/A.images', new TypeError('x is undefined'), [])).toBeNull();
    expect(unwritableError('/lib/A.images', refusal('EMFILE'), [])).toBeNull();
    expect(unwritableError('/lib/A.images', 'a string', [])).toBeNull();
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

    const ticks = stderr.trim().split('\n').filter(Boolean).map((l) => JSON.parse(l).progress);
    expect(ticks.length).toBeGreaterThan(0);
    for (const t of ticks) {
      expect(['parse', 'pictures', 'render']).toContain(t.stage);
      expect(t.percent).toBeGreaterThanOrEqual(0);
      expect(t.percent).toBeLessThanOrEqual(100);
    }

    // A bar that goes backwards is worse than no bar.
    const percents = ticks.map((t: { percent: number }) => t.percent);
    expect([...percents].sort((a, b) => a - b)).toEqual(percents);
    expect(percents.at(-1)).toBe(100);
  }, 60000);

  test('names each stage as it begins, even where the percent does not move', async () => {
    // The throttle drops a line that repeats the last percent, and a stage
    // can begin at the very percent the one before it ended on: images.pdf's
    // pages end at 60 and its pictures begin there, and its pictures end at
    // 85 where building the book begins. Dropping those lines would leave
    // the read-out naming the stage that already finished.
    const { stderr, exitCode } = await runCli([
      `${FIXTURES}images.pdf`, '-o', `${SCRATCH}/prog-pictures.epub`, '--no-fountain',
      '--json', '--progress',
    ]);
    expect(exitCode).toBe(0);
    const ticks: { stage: string; percent: number }[] = stderr.trim().split('\n').filter(Boolean)
      .map((l) => JSON.parse(l).progress);
    const firsts = ticks.filter((t, i) => i === 0 || ticks[i - 1].stage !== t.stage);
    expect(firsts.slice(1)).toEqual([
      { stage: 'pictures', percent: 60 },
      { stage: 'render', percent: 85 },
    ]);
    expect(ticks.at(-1)).toEqual({ stage: 'render', percent: 100 });
    // Still throttled within a stage: no two lines say the same thing.
    const said = ticks.map((t) => `${t.stage} ${t.percent}`);
    expect(new Set(said).size).toBe(said.length);
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
