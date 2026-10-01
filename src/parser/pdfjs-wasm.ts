// The image decoders pdf.js keeps OUTSIDE its bundle, carried into the
// compiled sidecar.
//
// pdf.js 6.x does not decode JPEG2000, JBIG2 or CCITT fax in `pdf.worker.mjs`.
// Each of those goes to a separate module it loads AT RUNTIME out of
// `pdfjs-dist/wasm/`, addressed by the `wasmUrl` API option: a directory, with
// a trailing slash, onto which it concatenates a fixed filename. Give it none
// and every such image fails — measured through the COMPILED binary on a real
// 110-page script-plus-lookbook whose pictures are all JPEG2000: `imageCount`
// absent, one warning reading "34 pictures could not be read out of the PDF",
// and under --debug a JpxError per picture over the cause,
// `#instantiateWasm: UnknownErrorException: Ensure that the `wasmUrl` API
// parameter is provided.` With this directory, the same binary on the same
// file carries 22 (34 decoded, 12 refused by artwork.ts as furniture) and
// warns about nothing.
//
// This is the same problem `pdfjs-shims.ts` solves for the worker, one step
// further out, and it is worth stating why a path into `node_modules` is not
// the answer: the shipped engine is `bun build --compile`d (CI, Homebrew) and
// then embedded in a signed .app, and none of those have a `node_modules`
// beside them. So the files are EMBEDDED with `type: "file"` and materialized
// into a private temp directory on first use.
//
// ── Why a temp directory rather than the embedded path itself
//
// Bun's `with { type: "file" }` yields a real path in a dev checkout and a
// virtual one (`/$bunfs/root/...`) in a compiled binary, and both can be READ.
// But the compiled binary RENAMES: `openjpeg_nowasm_fallback.js` embeds as
// `/$bunfs/root/openjpeg_nowasm_fallback-<hash>.js`. pdf.js builds its own
// filename onto our directory, so it would look for the un-hashed name and find
// nothing. `bun build --compile --asset-naming="[name].[ext]"` does suppress
// the hash, and the bunfs path is then importable — verified — but that puts
// the whole feature behind a build flag every future build path would have to
// remember, and forgetting it fails exactly the way this file exists to fix:
// silently, with the pictures gone. One code path, no flags, dev and shipped
// alike: copy the files out, hand pdf.js a directory it can address.
//
// Measured cost, once per process, compiled binary on this Mac (bun 1.3.14):
// 0.45ms to make the directory and copy all three files, 0.15ms to remove it at
// exit. End to end that is +2.5ms on a conversion of tests/fixtures/screenplay.pdf
// (79 -> 81ms), of which 1ms is the bigger binary starting up and not this
// directory at all; on the real scripts in the corpus, which run 210ms to 18s,
// it does not rise above the run-to-run noise.
//
// ── Why these three files and not the seven in that directory
//
// `openjpeg_nowasm_fallback.js` (JPEG2000), NOT `openjpeg.wasm`. This is the
// one surprise in the whole exercise, and it is worth stating plainly because
// the obvious fix is wrong: **the OpenJPEG wasm does not run under Bun at all.**
// `WebAssembly.compile` on it throws `CompileError: ... relaxed simd
// instructions not supported`, JavaScriptCore having no relaxed-SIMD support,
// and what actually decoded those 34 pictures when `wasmUrl` was first tried is
// pdf.js's own fallback: `#instantiateWasm` catches the failure and dynamically
// imports `${wasmUrl}openjpeg_nowasm_fallback.js`, a pure-JS build of the same
// decoder. Shipping the .wasm as well would only buy a 252 KB read and a failed
// compile before landing in the same place, so it is left out and the fetch
// fails one step earlier.
//
// `jbig2.wasm` (JBIG2 images and, through the same module, CCITTFaxDecode ones)
// DOES compile under Bun, so that one is carried as wasm: 105 KB against the
// 146 KB of its JS fallback, and the fast path. Its fallback rides along anyway
// — 146 KB against a 67 MB binary — because the sibling decoder above is proof
// that a runtime can refuse a wasm module, and the failure it would cause is the
// silent total loss this file exists to prevent.
//
// Two are deliberately absent, each for a reason read out of pdfjs-dist 6.2.108
// rather than guessed. (Re-read against 6.3.289 on 2026-10-01: both reasons
// still hold, the directory still holds the same seven files, and
// `openjpeg.wasm` still fails to compile under bun 1.3.14 at a relaxed-SIMD
// instruction while `jbig2.wasm` compiles.)
//
//   - `qcms_bg.wasm` (ICC colour management) is UNREACHABLE here. Its loader,
//     `IccColorSpace.setOptions`, sets `#useWasm = false` and returns whenever
//     `useWorkerFetch` is false (pdf.worker.mjs) — and `useWorkerFetch` is
//     false for every Node-like runtime, because pdf.js only defaults it true
//     when the binary-data factory is the DOM one (pdf.mjs). An ICCBased space
//     falls back to its /Alternate, which is what happens today.
//   - `quickjs-eval.wasm` runs JavaScript embedded in a PDF. Nothing outside
//     `pdf.sandbox.mjs` references it, we never import that module, and a
//     screenplay converter has no business executing a document's scripts.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import openjpegFallback from 'pdfjs-dist/wasm/openjpeg_nowasm_fallback.js' with { type: 'file' };
import jbig2Wasm from 'pdfjs-dist/wasm/jbig2.wasm' with { type: 'file' };
import jbig2Fallback from 'pdfjs-dist/wasm/jbig2_nowasm_fallback.js' with { type: 'file' };

/**
 * The decoders we carry, as `[the name pdf.js will ask for, where it is now]`.
 *
 * The FIRST half of each pair is not ours to choose: pdf.js concatenates these
 * exact names onto `wasmUrl` (`WasmImage._filename` and `._noWasmFilename`),
 * so a typo here is a decoder that is present and never found. Exported so a
 * test can pin the names against the package they came from.
 */
export const DECODER_FILES: readonly (readonly [string, string])[] = [
  ['openjpeg_nowasm_fallback.js', openjpegFallback],
  ['jbig2.wasm', jbig2Wasm],
  ['jbig2_nowasm_fallback.js', jbig2Fallback],
];

/**
 * The `wasmUrl` to hand pdf.js for a decoder directory: the directory as a
 * plain filesystem PATH with a `/` on the end. The same form on every
 * platform, Windows included. Each half of it is forced by a different
 * consumer, and a `file:` URL, the obvious "portable" choice, is wrong.
 *
 * Read out of pdfjs-dist 6.3.289 on 2026-10-01; the modern and legacy builds
 * agree. pdf.js does two things with it, and both are a plain string
 * concatenation, `${wasmUrl}${filename}`:
 *
 *   - It READS a `.wasm`. Under a Node-like runtime `getDocument` picks
 *     `NodeBinaryDataFactory` and leaves `useWorkerFetch` false (it only
 *     defaults true beside `DOMBinaryDataFactory`). So the worker's
 *     `WasmImage.#instantiateWasm` sends a "FetchBinaryData" message, and on
 *     this side `BaseBinaryDataFactory.fetch` hands the joined string to
 *     `NodeBinaryDataFactory._fetch`, which is `node_utils_fetchData`:
 *     `readFile` from `fs/promises` on the STRING. A string is always a path
 *     to `readFile`, never a URL. Measured on bun 1.3.14: a `file:///.../
 *     jbig2.wasm` string fails ENOENT. So a `file:` URL would lose the
 *     jbig2.wasm fast path on every platform, and silently, because pdf.js
 *     catches that failure and falls through to the JS fallback.
 *   - It IMPORTS a `*_nowasm_fallback.js`. `WasmImage.#getJsModule` is a bare
 *     dynamic `import()` of the joined string. Node's ESM loader would refuse
 *     `C:\...` there, reading `c:` as a URL scheme, but this engine only runs
 *     under Bun. Bun hands a plain-path specifier to its resolver as a path,
 *     not a URL (measured: a folder named with a space and a `#` imports
 *     fine, where URL parsing would have cut it at the `#`), and the
 *     resolver's absolute-path test is Zig's `std.fs.path.isAbsolute`, the
 *     HOST's rule, so `C:\...` is absolute on Windows (src/resolver/
 *     resolver.zig at bun-v1.3.14).
 *
 * So the path is the one string both consumers accept. The `/` is not
 * `path.sep` because `getFactoryUrlProp` throws on a `wasmUrl` that does not
 * END in `/`, on every platform: a backslash there would fail every
 * `getDocument`, not just the pictures. On Windows the result is a
 * mixed-separator path, `C:\...\screepub-pdfjs-x/jbig2.wasm`, and Windows
 * path rules take `/` and `\` alike as separators, so it names the file inside
 * the folder (tests/pdfjs-wasm.test.ts pins that with `path.win32`).
 *
 * What is read rather than run: nobody on this project has a Windows
 * machine, so the Windows half comes from the two codebases above and not
 * from a measurement. JPEG2000 is the format that would show it, since its
 * `.wasm` is never shipped (see the header) and every JPX picture goes
 * through the import.
 */
export function decoderBaseUrl(dir: string): string {
  return `${dir}/`;
}

/**
 * Copy the decoders into a directory of `makeDir`'s choosing, and return it
 * as the `wasmUrl` pdf.js wants (`decoderBaseUrl`) — or null if anything
 * refused.
 *
 * Null rather than a throw: a script with no JPEG2000, JBIG2 or CCITT image in
 * it — which is nearly all of them — converts perfectly without any of this,
 * and a read-only or full temp directory must not be the reason a screenplay
 * fails to become a book. The caller reports the loss instead; see
 * `ImageStats.decodersUnavailable`.
 *
 * `makeDir` is a parameter because the failure is the branch worth testing and
 * there is no other way to reach it: `mkdtempSync` under a real tmpdir does not
 * fail on demand.
 */
export function materializeDecoders(makeDir: () => string): string | null {
  try {
    const dir = makeDir();
    // read + write, and NOT the `copyFileSync` this obviously wants to be.
    // Inside a compiled binary the source is a bunfs path, which `copyfile(2)`
    // knows nothing about: `copyFileSync` fails ENOENT on the very path
    // `readFileSync` reads happily. Measured on bun 1.3.14, and it fails only
    // in the compiled binary — in a dev checkout the source is a real
    // node_modules file and copyFileSync works fine, so this is a tidy-up that
    // would pass every test here and break only what ships.
    for (const [name, src] of DECODER_FILES) writeFileSync(join(dir, name), readFileSync(src));
    return decoderBaseUrl(dir);
  } catch {
    return null;
  }
}

/** Where the decoders were put, or null. Memoized INCLUDING the null: the copy
 * is once per process, and so is the failure — a temp directory that refused
 * once will refuse again, and retrying per document would pay for it per page
 * of a lookbook. */
let dir: string | null | undefined;

/** The folder this process MADE, which is not always `dir`: a folder whose
 * copy failed halfway leaves `dir` null and is still ours to remove. */
let made: string | null = null;

/** Whether the `exit` hook is registered. Once per process, however many
 * folders `removePdfjsWasmDir` makes it forget: the hook removes whichever
 * one is current when the process ends. */
let exitHooked = false;

export function pdfjsWasmDir(): string | null {
  if (dir === undefined) {
    dir = materializeDecoders(() => {
      // mkdtemp, not a fixed name, and the unpredictability is the point: this
      // directory holds a JavaScript module that pdf.js will IMPORT AND RUN, so
      // a guessable path under a shared /tmp would let another user on the
      // machine leave their own `openjpeg_nowasm_fallback.js` there for us to
      // execute. mkdtempSync makes it 0700 with a random suffix, which closes
      // that; the price is that it cannot be reused between runs.
      made = mkdtempSync(join(tmpdir(), 'screepub-pdfjs-'));
      // Best-effort cleanup, and measured rather than assumed: `exit` fires and
      // the directory is removed for `bun src/cli.ts` and for the compiled
      // binary, which is every way the engine ships. It does NOT fire under
      // `bun test` (neither does `beforeExit`, bun 1.3.14), which is what
      // tests/remove-pdfjs-decoders.ts is for; see removePdfjsWasmDir. A hard
      // kill is the one case left to the OS temp directory's own pruning.
      if (!exitHooked) {
        process.on('exit', removePdfjsWasmDir);
        exitHooked = true;
      }
      return made;
    });
  }
  return dir;
}

/**
 * Remove the decoder folder this process made, if it made one, and forget it,
 * so a later `pdfjsWasmDir()` makes a fresh folder instead of handing pdf.js a
 * path that no longer exists.
 *
 * Two callers. The `exit` hook above, which covers every way the engine ships.
 * And tests/remove-pdfjs-decoders.ts, a `bun test` preload, because `exit` never
 * fires under `bun test`: without it every suite run left one ~700 KB
 * `screepub-pdfjs-*` folder in the REAL temp directory (measured 2026-10-01,
 * one new folder per run), and this repo once collected 23,000 `screepub-*`
 * folders that way (tests/temp-hygiene.test.ts).
 *
 * Synchronous, because an `exit` handler cannot await. Never throws: there is
 * nothing useful to do about a folder that will not go, and a throw at exit
 * would turn a successful conversion into a non-zero status.
 */
export function removePdfjsWasmDir(): void {
  const doomed = made;
  made = null;
  dir = undefined;
  if (doomed === null) return;
  try {
    rmSync(doomed, { recursive: true, force: true });
  } catch {
    // See above: nothing to do, and nothing to throw.
  }
}
