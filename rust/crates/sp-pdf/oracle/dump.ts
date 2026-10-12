// Transitional: what pdf.js hands the parser, page by page, as JSON. The
// answer key sp-pdf is measured against. Deleted at cutover with the rest of
// the TypeScript; reconcile with tools/oracle/dump.ts when that lands.
//
//   bun rust/crates/sp-pdf/oracle/dump.ts <in.pdf> <out.json> [pixels-dir]
//
// Run from the repo root. It opens the document exactly the way
// src/parser/extract.ts does (same getDocument options, same wasmUrl), reads
// each page's viewport, getTextContent items, and getOperatorList, and from
// the operator list keeps only the operators the parser walks. Image pixels
// are resolved the way images.ts resolves them and recorded as a SHA-256 of
// the data pdf.js hands over; with a pixels-dir, the raw bytes are written
// there too, for a pixel diff.
import '../../../../src/parser/pdfjs-shims';
import { getDocument, OPS } from 'pdfjs-dist/build/pdf.mjs';
import { pdfjsWasmDir } from '../../../../src/parser/pdfjs-wasm';
import { resolveImageData } from '../../../../src/parser/images';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [input, output, pixelsDir] = process.argv.slice(2);
if (!input || !output) {
  console.error('usage: dump.ts <in.pdf> <out.json> [pixels-dir]');
  process.exit(2);
}

const OP_NAME = new Map<number, string>(Object.entries(OPS).map(([k, v]) => [v as number, k]));

const bytes = new Uint8Array(readFileSync(input));
const wasmUrl = pdfjsWasmDir();
const pdf = await getDocument({ data: bytes, wasmUrl: wasmUrl ?? undefined }).promise;
if (pixelsDir) mkdirSync(pixelsDir, { recursive: true });

const pages = [];
for (let pageNum = 1; pageNum <= pdf.numPages; pageNum++) {
  const page = await pdf.getPage(pageNum);
  const viewport = page.getViewport({ scale: 1.0 });
  const text = await page.getTextContent();
  let ops: { fnArray: number[]; argsArray: unknown[] } | null = null;
  try {
    ops = (await page.getOperatorList()) as never;
  } catch {
    ops = null;
  }
  const items = (text.items as Record<string, unknown>[])
    .filter((it) => 'str' in it)
    .map((it) => ({
      str: it.str,
      dir: it.dir,
      width: it.width,
      height: it.height,
      transform: it.transform,
      fontName: it.fontName,
      hasEOL: it.hasEOL,
    }));
  const fonts: Record<string, string | null> = {};
  for (const it of items) {
    const id = it.fontName as string;
    if (!id || id in fonts) continue;
    try {
      const font = page.commonObjs.get(id) as { name?: string } | null;
      fonts[id] = font?.name == null ? null : String(font.name);
    } catch {
      fonts[id] = null;
    }
  }
  const opList: unknown[] = [];
  const images: unknown[] = [];
  const seenImg = new Set<string>();
  if (ops) {
    for (let i = 0; i < ops.fnArray.length; i++) {
      const fn = ops.fnArray[i]!;
      const args = ops.argsArray[i] as unknown[] | null;
      const op = OP_NAME.get(fn) ?? String(fn);
      switch (fn) {
        case OPS.save:
        case OPS.restore:
        case OPS.paintFormXObjectEnd:
        case OPS.endAnnotation:
          opList.push({ op });
          break;
        case OPS.transform:
          opList.push({ op, m: Array.from(args as number[]) });
          break;
        case OPS.paintFormXObjectBegin:
          opList.push({ op, m: args?.[0] ? Array.from(args[0] as number[]) : null });
          break;
        case OPS.beginAnnotation:
          opList.push({ op });
          break;
        case OPS.constructPath: {
          const bb = args?.[2] as ArrayLike<number> | null | undefined;
          opList.push({
            op,
            paint: OP_NAME.get(args?.[0] as number) ?? String(args?.[0]),
            bbox: bb ? Array.from(bb) : null,
          });
          break;
        }
        case OPS.paintImageXObject: {
          const [objId, w, h] = args as [string, number, number];
          opList.push({ op, objId, w, h });
          if (!seenImg.has(objId)) {
            seenImg.add(objId);
            const img = await resolveImageData(page.objs as never, page.commonObjs as never, objId);
            if (!img || !img.data) {
              images.push({ objId, missing: true });
            } else {
              const data = img.data as Uint8Array;
              const sha = createHash('sha256').update(data).digest('hex');
              images.push({ objId, width: img.width, height: img.height, kind: img.kind, len: data.length, sha256: sha });
              if (pixelsDir) writeFileSync(join(pixelsDir, `p${pageNum}-${objId}.raw`), data);
            }
          }
          break;
        }
        case OPS.paintInlineImageXObject:
        case OPS.paintImageMaskXObject:
        case OPS.paintSolidColorImageMask:
          opList.push({ op });
          break;
        case OPS.showText: {
          const glyphs = (args?.[0] ?? []) as unknown[];
          const t = glyphs
            .map((g) => (typeof g === 'number' ? '' : ((g as { unicode?: string })?.unicode ?? '')))
            .join('');
          opList.push({ op, text: t });
          break;
        }
        default:
          break;
      }
    }
  }
  pages.push({
    pageNum,
    width: viewport.width,
    height: viewport.height,
    view: page.view,
    rotate: page.rotate,
    items,
    fonts,
    ops: ops ? opList : null,
    images,
  });
}

writeFileSync(output, JSON.stringify({ numPages: pdf.numPages, pages }, null, 1));
