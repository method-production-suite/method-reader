// The oracle: every stage of the TypeScript engine, written down as the
// answer key for the Rust port (docs/superpowers/plans/2026-10-11-pure-rust-port.md).
// Transitional TypeScript, deleted at cutover with the rest of the engine.
//
//   bun tools/oracle/dump.ts <input.pdf|input.fountain> --out <dir>
//       [--options <file.json>] [--epoch <seconds>]
//
// Writes into <dir>:
//   pages.json       what pdf.js hands the parser, per page (PDF only)
//   lines.json       extract.ts's answer: the RawLines and the pictures (PDF only)
//   elements.json    the parser's answer (the CLI's --debug dump), before serialize
//   script.fountain  the serializer's answer (the input itself for a .fountain)
//   tokens.json      fountain-js tokens, exactly as the renderers receive them
//   book.epub, book.mobi, book.azw3, book.fdx, book.preview.html
//   images/          the picture folder the CLI wrote beside the .fountain
//   pixels/          each picture's source pixels as pdf.js handed them over,
//                    named <dataSha256>.bin after pages.json's hash (PDF only)
//   convert.json     the CLI's --json answer, with the scratch folder as $WORK
//   meta.json        what produced all of the above, and every file's sha256
//
// The books, script.fountain, elements.json and convert.json come from ONE
// run of the real CLI (src/cli.ts), so they are what a user gets, not a
// re-creation. That run is isolated: a scratch settings folder
// (SCREEPUB_CONFIG_DIR), a scratch library, a copy of the input in a folder
// of its own (so no <script>.screepub.json beside the real input applies),
// TZ=UTC, and the clock frozen at --epoch (tools/oracle/freeze-clock.ts),
// so a second dump of the same input is byte-identical.
//
// pages.json and lines.json come from this process, reading the PDF the way
// src/parser/extract.ts does. No engine hook is needed for either: pdf.js is
// asked the same questions extract.ts asks, and extractDocument is exported.
// As a guard against the two drifting, the parse of lines.json is checked
// against the CLI's elements.json and the dump fails if they differ.
import '../../src/parser/pdfjs-shims';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import {
  copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync,
  realpathSync, rmSync, statSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, extname, join, resolve } from 'node:path';
import { Fountain } from 'fountain-js';
import { getDocument, OPS } from 'pdfjs-dist/build/pdf.mjs';
import { extractDocument } from '../../src/parser/extract';
import { resolveImageData, type OpList, type PdfImage } from '../../src/parser/images';
import { parseLines } from '../../src/parser/index';
import { pdfjsWasmDir } from '../../src/parser/pdfjs-wasm';

const REPO = resolve(import.meta.dir, '../..');
const CLI = join(REPO, 'src/cli.ts');
const FREEZE = join(REPO, 'tools/oracle/freeze-clock.ts');
/** 2026-01-01T00:00:00Z. Matches freeze-clock.ts and the xtask default. */
export const DEFAULT_EPOCH = 1767225600;
/** Bumped when the dump's own shape changes, so a reader can tell. */
export const ORACLE_FORMAT = 1;

const sha256 = (b: Uint8Array | string) => createHash('sha256').update(b).digest('hex');

// ── pdf.js facts ─────────────────────────────────────────

/** OPS number → name, for the operators the parser walks. */
const OP_NAMES = new Map<number, string>(
  Object.entries(OPS as Record<string, number>).map(([name, n]) => [n, name]),
);

/**
 * The operators extract.ts and images.ts read, and only those, in order:
 * save/restore/transform track the CTM for underlines and pictures,
 * constructPath is a candidate underline, showText is the glyph stream the
 * phantom-space repair checks words against, the form and annotation pairs
 * move and fence the CTM for pictures, and paintImageXObject is a picture.
 */
function walkOps(ops: OpList): unknown[] {
  const out: unknown[] = [];
  for (let i = 0; i < ops.fnArray.length; i++) {
    const fn = ops.fnArray[i];
    const args = ops.argsArray[i] as unknown[] | null | undefined;
    switch (fn) {
      case OPS.save:
      case OPS.restore:
      case OPS.paintFormXObjectEnd:
      case OPS.beginAnnotation:
      case OPS.endAnnotation:
        out.push({ op: OP_NAMES.get(fn) });
        break;
      case OPS.transform:
        out.push({ op: 'transform', matrix: Array.from(args as ArrayLike<number>) });
        break;
      case OPS.paintFormXObjectBegin: {
        const matrix = (args?.[0] as ArrayLike<number> | null | undefined) ?? null;
        out.push({ op: 'paintFormXObjectBegin', matrix: matrix ? Array.from(matrix) : null });
        break;
      }
      case OPS.constructPath: {
        // [paint op, packed path data, bbox]; the parser reads the first and
        // the last and never decodes the path itself.
        const paint = args?.[0];
        const bbox = args?.[2] as ArrayLike<number> | null | undefined;
        out.push({
          op: 'constructPath',
          paint: typeof paint === 'number' ? (OP_NAMES.get(paint) ?? paint) : null,
          bbox: bbox ? Array.from(bbox) : null,
        });
        break;
      }
      case OPS.showText: {
        // Glyph unicode joined, numbers (positioning) as nothing: exactly
        // the string repairPhantomSpaces builds from the run.
        const glyphs = args?.[0];
        const text = Array.isArray(glyphs)
          ? glyphs.map((g) => (typeof g === 'number' ? '' : ((g as { unicode?: string })?.unicode ?? ''))).join('')
          : null;
        out.push({ op: 'showText', text });
        break;
      }
      case OPS.paintImageXObject:
        out.push({ op: 'paintImageXObject', objId: args?.[0], width: args?.[1], height: args?.[2] });
        break;
      default:
        break;
    }
  }
  return out;
}

interface PageObjs {
  objs: { has(k: string): boolean; get(k: string, cb: (o: PdfImage | null) => void): void };
  commonObjs: { has(k: string): boolean; get(k: string, cb?: (o: unknown) => void): unknown };
}

/** pages.json: per page, the facts extract.ts reads off pdf.js. With
 * `pixelsDir`, each picture's bytes are also written there as
 * `<dataSha256>.bin`, the layout sp-pdf and sp-parser read pixels from. */
async function pdfFacts(bytes: Uint8Array, pixelsDir?: string): Promise<unknown> {
  const wasmUrl = pdfjsWasmDir();
  const pdf = await getDocument({ data: bytes.slice(), wasmUrl: wasmUrl ?? undefined }).promise;
  const pages: unknown[] = [];
  for (let pageNum = 1; pageNum <= pdf.numPages; pageNum++) {
    const page = await pdf.getPage(pageNum);
    const viewport = page.getViewport({ scale: 1.0 });
    const content = await page.getTextContent();
    // Copied field by field BEFORE anything else reads them: extract.ts
    // mutates its own items (styles, underlines, repaired strings), and this
    // is the untouched pdf.js answer.
    const items = (content.items as Record<string, unknown>[])
      .filter((it) => 'str' in it && 'transform' in it)
      .map((it) => ({
        str: it.str,
        dir: it.dir,
        width: it.width,
        height: it.height,
        transform: Array.from(it.transform as number[]),
        fontName: it.fontName,
        hasEOL: it.hasEOL,
      }));
    let ops: OpList | null = null;
    try {
      ops = (await page.getOperatorList()) as OpList;
    } catch {
      ops = null;
    }
    // Font names resolve only after getOperatorList, the same order
    // extract.ts relies on. null is an unresolved font, which the parser
    // reads as plain and unbucketed.
    const store = page as unknown as PageObjs;
    const fonts: Record<string, string | null> = {};
    for (const it of items) {
      const id = it.fontName as string | undefined;
      if (!id || id in fonts) continue;
      try {
        const font = store.commonObjs.get(id) as { name?: string } | null;
        fonts[id] = font?.name === undefined ? null : String(font.name);
      } catch {
        fonts[id] = null;
      }
    }
    const walked = ops ? walkOps(ops) : null;
    // The pixels of each picture painted here, by the object id the paint
    // names, as pdf.js hands them over (before toRgb). Hashed rather than
    // inlined: sp-pdf decodes its own and compares.
    const images: Record<string, unknown> = {};
    for (const o of (walked ?? []) as { op: string; objId?: unknown }[]) {
      if (o.op !== 'paintImageXObject' || typeof o.objId !== 'string' || o.objId in images) continue;
      const img = await resolveImageData(store.objs, store.commonObjs as never, o.objId);
      if (img?.data && pixelsDir) {
        const data = new Uint8Array(img.data.buffer, img.data.byteOffset, img.data.byteLength);
        mkdirSync(pixelsDir, { recursive: true });
        writeFileSync(join(pixelsDir, `${sha256(data)}.bin`), data);
      }
      images[o.objId] = img === null
        ? null
        : {
            width: img.width,
            height: img.height,
            kind: img.kind,
            dataLength: img.data ? img.data.length : null,
            dataSha256: img.data ? sha256(new Uint8Array(img.data.buffer, img.data.byteOffset, img.data.byteLength)) : null,
          };
    }
    pages.push({
      pageNum,
      width: viewport.width,
      height: viewport.height,
      items,
      fonts,
      ops: walked,
      images,
    });
  }
  return { numPages: pdf.numPages, pages };
}

// ── the CLI run ──────────────────────────────────────────

/** Every string in `v` with the scratch folder spelled `$WORK`. */
function normalizePaths(v: unknown, roots: string[]): unknown {
  if (typeof v === 'string') {
    let s = v;
    for (const r of roots) s = s.split(r).join('$WORK');
    return s;
  }
  if (Array.isArray(v)) return v.map((x) => normalizePaths(x, roots));
  if (v && typeof v === 'object') {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, normalizePaths(x, roots)]));
  }
  return v;
}

export interface DumpOptions {
  input: string;
  out: string;
  options?: string;
  epoch?: number;
}

export async function dump({ input, out, options, epoch = DEFAULT_EPOCH }: DumpOptions): Promise<void> {
  const ext = extname(input).toLowerCase();
  const isPdf = ext === '.pdf';
  const work = mkdtempSync(join(tmpdir(), 'screepub-oracle-'));
  try {
    const inDir = join(work, 'in');
    const outDir = join(work, 'out');
    for (const d of [inDir, outDir, join(work, 'config'), join(work, 'library')]) mkdirSync(d);
    const copy = join(inDir, basename(input));
    copyFileSync(input, copy);
    // A .fountain's pictures sit in <stem>.images beside it (src/library-images.ts),
    // and the CLI reads them from there, so they travel with the copy. Its
    // <stem>.screepub.json deliberately does not: the dump is of the
    // defaults plus --options, never of somebody's saved tuning.
    if (!isPdf) {
      const pictures = `${input.slice(0, input.length - ext.length)}.images`;
      if (existsSync(pictures)) {
        const to = `${copy.slice(0, copy.length - ext.length)}.images`;
        mkdirSync(to);
        for (const f of readdirSync(pictures)) copyFileSync(join(pictures, f), join(to, f));
      }
    }

    const args = [
      '--preload', FREEZE, CLI, copy,
      '-o', join(outDir, 'book.epub'),
      '--mobi',
      '--azw3', join(outDir, 'book.azw3'),
      '--fdx', join(outDir, 'book.fdx'),
      '--preview-html', join(outDir, 'book.preview.html'),
      '--debug',
      '--json',
    ];
    if (isPdf) args.push('--fountain', join(outDir, 'script.fountain'));
    if (options) args.push('--options', resolve(options));
    const run = spawnSync(process.execPath, args, {
      cwd: REPO,
      env: {
        ...process.env,
        SCREEPUB_CONFIG_DIR: join(work, 'config'),
        SCREEPUB_LIBRARY: join(work, 'library'),
        // Nothing here starts Calibre; a guard path makes sure nothing could.
        CALIBRE_CONFIG_DIRECTORY: '/dev/null/screepub-oracle-calibre-guard',
        SOURCE_DATE_EPOCH: String(epoch),
        TZ: 'UTC',
      },
      maxBuffer: 256 * 1024 * 1024,
    });
    if (run.error) throw run.error;

    rmSync(out, { recursive: true, force: true });
    mkdirSync(out, { recursive: true });

    const roots = [work, realpathSync(work)].sort((a, b) => b.length - a.length);
    const stdout = run.stdout.toString('utf8').trim();
    let answer: unknown;
    try {
      answer = JSON.parse(stdout);
    } catch {
      throw new Error(`the CLI did not answer JSON (exit ${run.status}):\n${stdout}\n${run.stderr.toString('utf8')}`);
    }
    writeJson(join(out, 'convert.json'), normalizePaths(answer, roots));

    // The CLI's own files, renamed to fixed names.
    const moves: [string, string][] = [
      ['book.epub', 'book.epub'],
      ['book.mobi', 'book.mobi'],
      ['book.azw3', 'book.azw3'],
      ['book.fdx', 'book.fdx'],
      ['book.preview.html', 'book.preview.html'],
      ['book.elements.json', 'elements.json'],
    ];
    if (isPdf) moves.push(['script.fountain', 'script.fountain']);
    for (const [from, to] of moves) {
      const p = join(outDir, from);
      if (existsSync(p)) copyFileSync(p, join(out, to));
    }
    const pictures = join(outDir, 'script.images');
    if (existsSync(pictures)) {
      mkdirSync(join(out, 'images'));
      for (const f of readdirSync(pictures).sort()) copyFileSync(join(pictures, f), join(out, 'images', f));
    }

    // The tokens the renderers received: fountain-js over the same text.
    const fountainText = isPdf
      ? (existsSync(join(out, 'script.fountain')) ? readFileSync(join(out, 'script.fountain'), 'utf8') : null)
      : readFileSync(input, 'utf8');
    if (!isPdf) writeFileSync(join(out, 'script.fountain'), fountainText!);
    if (fountainText !== null) {
      writeJson(join(out, 'tokens.json'), new Fountain().parse(fountainText, true).tokens);
    }

    if (isPdf) {
      const bytes = new Uint8Array(readFileSync(input));
      writeJson(join(out, 'pages.json'), await pdfFacts(bytes, join(out, 'pixels')));
      const doc = await extractDocument(bytes.slice());
      writeJson(join(out, 'lines.json'), {
        pageCount: doc.pageCount,
        spacingRepairs: doc.spacingRepairs,
        lines: doc.lines,
        images: doc.images.map(({ bytes: png, ...rest }) => ({ ...rest, sha256: sha256(png) })),
        imageStats: doc.imageStats,
      });
      // The two halves of the dump must describe one conversion.
      const elementsPath = join(out, 'elements.json');
      if (existsSync(elementsPath)) {
        const ours = JSON.stringify(parseLines(doc.lines, doc.images));
        const cli = JSON.stringify(JSON.parse(readFileSync(elementsPath, 'utf8')));
        if (ours !== cli) {
          throw new Error(`${input}: the parse of lines.json differs from the CLI's elements.json`);
        }
      }
    }

    writeJson(join(out, 'meta.json'), {
      oracleFormat: ORACLE_FORMAT,
      input: basename(input),
      inputSha256: sha256(readFileSync(input)),
      options: options ? JSON.parse(readFileSync(options, 'utf8')) : null,
      sourceDateEpoch: epoch,
      exitCode: run.status,
      versions: {
        screepub: JSON.parse(readFileSync(join(REPO, 'package.json'), 'utf8')).version,
        pdfjs: JSON.parse(readFileSync(join(REPO, 'node_modules/pdfjs-dist/package.json'), 'utf8')).version,
        fountainJs: JSON.parse(readFileSync(join(REPO, 'node_modules/fountain-js/package.json'), 'utf8')).version,
        bun: Bun.version,
      },
      files: listFiles(out),
    });
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

/** Indented JSON, with arrays of plain numbers (transforms, boxes) kept on
 * one line so a page of text items stays readable in a diff. */
function writeJson(path: string, v: unknown): void {
  const text = JSON.stringify(v, null, 2).replace(
    /\[\s+(-?[\d.e+-]+(?:,\s+-?[\d.e+-]+)*)\s+\]/g,
    (_, inner: string) => `[${inner.split(/,\s+/).join(', ')}]`,
  );
  writeFileSync(path, `${text}\n`);
}

/** Every file under `dir` (meta.json aside), with its size and sha256. */
function listFiles(dir: string, prefix = ''): Record<string, { bytes: number; sha256: string }> {
  const out: Record<string, { bytes: number; sha256: string }> = {};
  for (const name of readdirSync(dir).sort()) {
    const rel = prefix ? `${prefix}/${name}` : name;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) Object.assign(out, listFiles(p, rel));
    else if (rel !== 'meta.json') out[rel] = { bytes: statSync(p).size, sha256: sha256(readFileSync(p)) };
  }
  return out;
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  let input: string | undefined;
  let out: string | undefined;
  let options: string | undefined;
  let epoch: number | undefined;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--out') out = argv[++i];
    else if (a === '--options') options = argv[++i];
    else if (a === '--epoch') epoch = Number(argv[++i]);
    else if (a === '-h' || a === '--help') input = undefined, out = undefined, i = argv.length;
    else input = a;
  }
  if (!input || !out) {
    process.stderr.write('usage: bun tools/oracle/dump.ts <input.pdf|input.fountain> --out <dir> [--options <file.json>] [--epoch <seconds>]\n');
    process.exit(2);
  }
  await dump({ input: resolve(input), out: resolve(out), options, epoch });
  // pdf.js keeps its worker alive; the dump is finished.
  process.exit(0);
}
