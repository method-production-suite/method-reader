// src/parser/pdfjs-wasm.ts on its own: the names pdf.js asks for, the string it
// is handed, and the folder it leaves behind. Whether a JPEG2000 picture then
// actually decodes is tests/jpx-fixture.test.ts, end to end.
import { afterAll, describe, expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, win32 } from 'node:path';
import {
  DECODER_FILES,
  decoderBaseUrl,
  pdfjsWasmDir,
  removePdfjsWasmDir,
} from '../src/parser/pdfjs-wasm';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-pdfjs-wasm-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const ROOT = new URL('..', import.meta.url).pathname;
const WORKER = Bun.resolveSync('pdfjs-dist/build/pdf.worker.mjs', ROOT);
const PACKAGE_WASM = join(dirname(WORKER), '..', 'wasm');

describe('the decoder files', () => {
  test('every name we carry is one the installed pdf.js asks for', () => {
    // pdf.js builds the file name itself from fields on each decoder class
    // (`_filename` and `_noWasmFilename` on JpxImage and JBig2CCITTFaxImage),
    // so a name that drifts on a pdf.js bump is a decoder that ships and is
    // never found. Read out of the worker that will do the asking, not out of
    // a list copied from it.
    const source = readFileSync(WORKER, 'utf8');
    const asked = [...source.matchAll(/\b_(?:noWasmF|f)ilename = "([^"]+)"/g)].map((m) => m[1]);
    for (const [name] of DECODER_FILES) expect(asked).toContain(name);
  });

  test('every name we carry exists in the installed package', () => {
    // The other half of a rename: `import ... with { type: "file" }` would
    // fail the build, but only for a build someone runs. This fails the suite.
    for (const [name] of DECODER_FILES) {
      expect(existsSync(join(PACKAGE_WASM, name))).toBe(true);
    }
  });

  test('and every embedded source is that same file on disk', () => {
    // In a dev checkout the `type: "file"` import is the real node_modules
    // path. (In a compiled binary it is a renamed /$bunfs one, which is why
    // the files are copied out at all; see the header of pdfjs-wasm.ts.)
    for (const [name, src] of DECODER_FILES) {
      expect(basename(src)).toBe(name);
      expect(existsSync(src)).toBe(true);
    }
  });
});

describe('decoderBaseUrl: the wasmUrl string', () => {
  test('on Windows it is the drive path itself, plus the slash pdf.js requires', () => {
    // A plain PATH and not a `file:` URL, on Windows too. pdf.js does two
    // things with `${wasmUrl}${filename}` under Bun: `readFile` on the string
    // (NodeBinaryDataFactory), which reads a `file:` URL string as a relative
    // path and fails, and a bare `import()` (WasmImage.#getJsModule), which
    // Bun resolves as a path when the host calls it absolute, as Windows does
    // `C:\...`. The full reasoning, with the pdf.js and Bun functions it was
    // read out of, is on decoderBaseUrl.
    //
    // And `/` rather than `\`: getFactoryUrlProp throws on a wasmUrl that
    // does not END in `/`, on every platform.
    const dir = 'C:\\Users\\reader\\AppData\\Local\\Temp\\screepub-pdfjs-AbC123';
    const base = decoderBaseUrl(dir);
    expect(base).toBe('C:\\Users\\reader\\AppData\\Local\\Temp\\screepub-pdfjs-AbC123/');

    // What pdf.js then builds names the file INSIDE that folder, by Windows's
    // own path rules: absolute, and the same file a backslash join names.
    const wasm = `${base}jbig2.wasm`;
    expect(win32.isAbsolute(wasm)).toBe(true);
    expect(win32.normalize(wasm)).toBe(win32.join(dir, 'jbig2.wasm'));
  });

  test('on POSIX it is the path plus the slash', () => {
    expect(decoderBaseUrl('/tmp/screepub-pdfjs-AbC123')).toBe('/tmp/screepub-pdfjs-AbC123/');
  });

  test('both things pdf.js does with it work on the real folder', async () => {
    // Exactly what pdf.js does, on exactly the string it gets. A `file:` URL
    // here would pass the import and fail the read, which is the trap.
    const base = pdfjsWasmDir()!;
    // NodeBinaryDataFactory: readFile on the joined STRING. `\0asm` is the
    // wasm magic number, so this is the module and not merely some file.
    const wasm = await readFile(`${base}jbig2.wasm`);
    expect([...wasm.subarray(0, 4)]).toEqual([0x00, 0x61, 0x73, 0x6d]);
    // WasmImage.#getJsModule: a bare import() of the joined string, whose
    // default export pdf.js then calls.
    const mod = await import(`${base}openjpeg_nowasm_fallback.js`);
    expect(typeof mod.default).toBe('function');
  });
});

describe('removePdfjsWasmDir', () => {
  test('removes the folder this process made, and the next ask makes a fresh one', () => {
    const first = pdfjsWasmDir()!;
    expect(existsSync(first)).toBe(true);
    removePdfjsWasmDir();
    expect(existsSync(first)).toBe(false);
    // Forgotten, not left pointing at a folder that is gone: that would hand
    // pdf.js a path with nothing in it, and every JPEG2000 picture after it
    // would fail without a word.
    const second = pdfjsWasmDir()!;
    expect(second).not.toBe(first);
    expect(existsSync(second)).toBe(true);
    expect(basename(second)).toStartWith('screepub-pdfjs-');
  });

  test('is safe with nothing to remove, and twice over', () => {
    removePdfjsWasmDir();
    expect(() => removePdfjsWasmDir()).not.toThrow();
  });

  test('registers its exit hook once, however often the folder is remade', () => {
    // Once per process, not once per folder: a hook per folder would pile up
    // listeners for as long as a process keeps remaking it. And it has to be
    // THIS function, which is what the plain-bun tests below then watch run.
    pdfjsWasmDir();
    const before = process.listenerCount('exit');
    for (let i = 0; i < 3; i++) {
      removePdfjsWasmDir();
      pdfjsWasmDir();
    }
    expect(process.listenerCount('exit')).toBe(before);
    expect(process.listeners('exit').filter((l) => l === removePdfjsWasmDir)).toHaveLength(1);
  });
});

/** Run a process to the end. stderr is kept only so that a failing assertion
 * on the result prints it; stdout only when the caller reads it. */
async function finish(
  cmd: string[],
  opts: { cwd: string; tmp: string; stdout?: boolean },
): Promise<{ code: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn(cmd, {
    cwd: opts.cwd,
    env: { ...process.env, TMPDIR: opts.tmp },
    stdout: opts.stdout ? 'pipe' : 'ignore',
    stderr: 'pipe',
  });
  const [code, stdout, stderr] = await Promise.all([
    proc.exited,
    opts.stdout ? new Response(proc.stdout).text() : '',
    new Response(proc.stderr).text(),
  ]);
  return { code, stdout, stderr };
}

// THE cleanup that ships: `bun src/cli.ts` and the compiled binary remove the
// folder from an `exit` hook, and nothing else does. Under `bun test` the
// preload removes it instead (`exit` never fires there), which is exactly why
// these run the engine under PLAIN bun: in-process, a hook that did nothing
// would be covered up by the preload and every test above would stay green.
// Each case gets its own temp folder inside SCRATCH, so "empty afterwards" is
// about that one process and nothing else on the machine.
describe('the exit hook, in a process that is not bun test', () => {
  const probe = (name: string, ending: string) => {
    const file = join(SCRATCH, name);
    writeFileSync(file, [
      `import { pdfjsWasmDir } from '${ROOT}src/parser/pdfjs-wasm';`,
      'console.log(pdfjsWasmDir());',
      ending,
    ].join('\n'));
    return file;
  };

  for (const [label, ending, code] of [
    ['when the script runs off its end', '', 0],
    ['when it calls process.exit, as the CLI does', 'process.exit(3);', 3],
  ] as const) {
    test(`removes the folder ${label}`, async () => {
      const tmp = join(SCRATCH, `exit-${code}-tmp`);
      mkdirSync(tmp);
      const run = await finish([process.execPath, probe(`exit-${code}.ts`, ending)], {
        cwd: SCRATCH,
        tmp,
        stdout: true,
      });
      expect(run).toMatchObject({ code });
      // Made, and made in THIS temp folder: otherwise "gone afterwards" would
      // pass for a process that never made one.
      const made = run.stdout.trim();
      expect(made).toStartWith(`${tmp}/screepub-pdfjs-`);
      expect(existsSync(made)).toBe(false);
      expect(readdirSync(tmp)).toEqual([]);
    });
  }
});

describe('a bun test run', () => {
  test('leaves no decoder folder in the temp folder', async () => {
    // The end-to-end check that bunfig.toml still preloads
    // tests/remove-pdfjs-decoders.ts: `exit` never fires under bun test, so
    // without that line every run leaves a folder behind. A nested run with
    // its temp folder pointed into SCRATCH, so the count is of that run alone
    // and not of whatever else on the machine shares the real one. cwd is the
    // repo, so the nested run reads this repo's bunfig.toml.
    const tmp = join(SCRATCH, 'nested-tmp');
    mkdirSync(tmp);
    // Where the nested test writes down the folder it made: outside its temp
    // folder, so the record survives the cleanup it is evidence for. The exit
    // code cannot be that evidence, because bun test exits 0 on a file that
    // registers no test at all (bun 1.3.14), and an empty temp folder is what
    // a run that never made one leaves too.
    const record = join(SCRATCH, 'nested-made.txt');
    const file = join(SCRATCH, 'opens-decoders.test.ts');
    writeFileSync(file, [
      "import { test } from 'bun:test';",
      "import { writeFileSync } from 'node:fs';",
      `import { pdfjsWasmDir } from '${ROOT}src/parser/pdfjs-wasm';`,
      `test('unpacks the decoders', () => writeFileSync(${JSON.stringify(record)}, pdfjsWasmDir()!));`,
    ].join('\n'));
    const run = await finish([process.execPath, 'test', file], { cwd: ROOT, tmp });
    expect(run).toMatchObject({ code: 0 });
    const made = readFileSync(record, 'utf8');
    expect(made).toStartWith(`${tmp}/screepub-pdfjs-`);
    expect(existsSync(made)).toBe(false);
    expect(readdirSync(tmp)).toEqual([]);
  });
});
