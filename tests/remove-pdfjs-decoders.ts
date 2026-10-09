// Preloaded before every test file by bunfig.toml's [test] section. It removes
// the folder of pdf.js image decoders the engine unpacks into the system temp
// folder (src/parser/pdfjs-wasm.ts), once, after the last test file.
//
// The engine removes that folder itself from an `exit` handler, which covers
// every way it ships (`bun src/cli.ts` and the compiled binary). But `exit`
// never fires under `bun test` (bun 1.3.14), so without this file each suite
// run that opened a document left one ~700 KB `screepub-pdfjs-*` folder behind
// in the REAL temp folder. Measured 2026-10-01: three runs of
// `bun test tests/images.test.ts tests/jpx-fixture.test.ts` left three.
//
// Why a preload and not the scratch-folder rule every test file follows
// (tests/temp-hygiene.test.ts): the folder is made by the ENGINE, once per
// process and memoized, on behalf of whichever test file opens a document
// first. No single test file owns it, so no test file's afterAll can remove
// it without pulling it out from under the files that run after.
//
// An afterAll in a preload runs once, after the last test file of the run
// (tests/isolate-calibre-config.ts checked that on bun 1.3.14). A run cut
// short by --bail or Ctrl-C skips it and leaves the folder, the same as every
// other scratch folder in tests/. Subprocesses a test starts are not this
// file's concern: each is its own process, and its own `exit` handler removes
// its own folder.
//
// The engine module is imported INSIDE the afterAll, never at the top. A
// preload's top-level imports load before every test file, so a pdf.js
// update that renamed one of the decoder files pdfjs-wasm.ts embeds would
// fail EVERY test file with "Unhandled error between tests" (checked
// 2026-10-01: tests/png.test.ts, which has nothing to do with pdf.js, went
// red that way). Imported here, the rename fails only the files that use the
// module, which is where the fault is. By the time this runs, the import
// returns the instance the test files used, folder and all; in a run where no
// file used it, it loads fresh with nothing to remove.
//
// And an import that fails here is let go, not rethrown: a module that cannot
// load never made a folder, and every file that imports it has already said
// why. Rethrown, it lands as an "(unnamed)" failure under whichever file ran
// last, pointing nowhere near the cause.
import { afterAll } from 'bun:test';

afterAll(async () => {
  let engine: typeof import('../src/parser/pdfjs-wasm');
  try {
    engine = await import('../src/parser/pdfjs-wasm');
  } catch {
    return;
  }
  engine.removePdfjsWasmDir();
});
