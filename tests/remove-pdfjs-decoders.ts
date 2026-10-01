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
import { afterAll } from 'bun:test';
import { removePdfjsWasmDir } from '../src/parser/pdfjs-wasm';

afterAll(() => removePdfjsWasmDir());
