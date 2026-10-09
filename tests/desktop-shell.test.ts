import { describe, test, expect } from 'bun:test';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import { SIDECAR_BASENAME } from '../tools/sidecar-targets';

const REPO = new URL('..', import.meta.url).pathname;
const RUST_DIR = join(REPO, 'desktop', 'src-tauri');
const CARGO = readFileSync(join(REPO, 'desktop', 'src-tauri', 'Cargo.toml'), 'utf8');
const CONFIG = JSON.parse(
  readFileSync(join(REPO, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8'),
);

// Recursive, and rooted at the whole crate rather than only src/: a
// `build.rs` beside Cargo.toml, or a sibling crate someone drops in its own
// subdirectory (e.g. `mod brain;` pulled in from
// `desktop/src-tauri/brain/src/lib.rs`), is exactly the "one more small
// thing in Rust" shape a submodule under src/ already is — and a scan that
// stopped at src/ left both entirely ungoverned by every guard below.
// (Demonstrated: appending code to build.rs, and adding a brain/ crate,
// each passed every test here before this glob was widened to the crate
// root.)
//
// The three excluded directories are cargo/tauri build OUTPUT, never
// source a person wrote by hand: target/ (cargo's build dir, full of
// generated .rs files from build scripts), gen/ (tauri-cli's generated
// schemas) and binaries/ (the compiled sidecar). All three are gitignored
// for the same reason — see root .gitignore.
const EXCLUDED_DIRS = new Set(['target', 'gen', 'binaries']);
const rustFiles = (readdirSync(RUST_DIR, { recursive: true }) as string[]).filter((f) => {
  if (!f.endsWith('.rs')) return false;
  const segments = f.split(/[\\/]/);
  return !segments.some((seg) => EXCLUDED_DIRS.has(seg));
});
const rustSources = rustFiles.map((f) => ({
  name: f,
  text: readFileSync(join(RUST_DIR, f), 'utf8'),
}));

/**
 * Source with every comment's TEXT removed but its code kept, for guards
 * that must judge what the Rust *does* rather than what it says about
 * itself. A doc comment explaining "we deliberately don't depend on
 * serde_json" or spelling out what a scene is for the reader's benefit is
 * exactly the kind of sentence these guards would otherwise trip on —
 * words that describe the rule, not code that breaks it.
 *
 * This tracks string-literal state (not just a line-oriented scan), because
 * an ordinary future edit — a URL or path inside a format! string — can put
 * "//" or "/*" inside a string literal. A scan blind to quoting would treat
 * the rest of that string, or an arbitrary span after a stray "/*" inside a
 * string, as a comment and hide real code from every guard that reads
 * stripped text. Escaped quotes (`\"`) are honoured so a string is not
 * closed early.
 */
function stripComments(text: string): string {
  let out = '';
  let i = 0;
  let inString = false;
  let inBlockComment = false;
  while (i < text.length) {
    const ch = text[i];
    if (inBlockComment) {
      if (ch === '*' && text[i + 1] === '/') {
        inBlockComment = false;
        i += 2;
        continue;
      }
      out += ch === '\n' ? '\n' : ''; // keep line breaks; content is gone
      i += 1;
      continue;
    }
    if (inString) {
      out += ch;
      if (ch === '\\' && i + 1 < text.length) {
        out += text[i + 1];
        i += 2;
        continue;
      }
      if (ch === '"') inString = false;
      i += 1;
      continue;
    }
    if (ch === '"') {
      inString = true;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === '/' && text[i + 1] === '/') {
      const nl = text.indexOf('\n', i);
      i = nl === -1 ? text.length : nl; // resume at the newline itself
      continue;
    }
    if (ch === '/' && text[i + 1] === '*') {
      inBlockComment = true;
      i += 2;
      continue;
    }
    out += ch;
    i += 1;
  }
  return out;
}

describe('Rust is a window, not a brain', () => {
  test('the crate cannot parse the engine’s answer', () => {
    // THE load-bearing assertion of this whole piece. The engine's stdout
    // is an opaque string on its way to the frontend, and no amount of
    // well-meant Rust may start branching on what is inside it. `serde`
    // itself is allowed: it is how the ipc layer encodes arguments, and it
    // cannot read an arbitrary JSON document.
    //
    // AMENDED 2026-09-21, deliberately. This used to assert serde_json was
    // not in Cargo.toml at all. Then the updater plugin arrived: its config
    // has to live under `plugins` in tauri.conf.json (the plugin has no
    // Rust setter for its endpoints), and `tauri::generate_context!` embeds
    // any `plugins` block as `::serde_json::Value` literals
    // (tauri-utils/src/tokens.rs, json_value_lit), so the build fails
    // without the crate. Measured, not assumed.
    //
    // So the guarantee moves from "not linked" to "linked for the
    // generator, and no source here may name it", which is the half of the
    // rule that ever did the work: a parser nobody calls parses nothing.
    // The dependency line must carry that reason in the comment directly
    // above it, so it cannot later be read as permission.
    const dep = /^serde_json = "1"$/m.exec(CARGO);
    expect(dep).not.toBeNull();
    const commentAbove = CARGO.slice(0, dep!.index)
      .split('\n')
      .reverse()
      .slice(1) // the empty tail after the final newline
      .filter((_line, i, arr) => arr.slice(0, i + 1).every((l) => l.startsWith('#')))
      .join('\n');
    expect(commentAbove).toContain('generate_context');
    expect(commentAbove).toContain('no source');
    // Exactly one such line, and nothing widening it (no features, no path).
    expect(CARGO.match(/^\s*serde_json\s*=/gm)).toHaveLength(1);
    //
    // This reads comment-stripped code: a doc comment that mentions
    // `serde_json` to EXPLAIN why it is unused (as sidecar.rs's does) must
    // not trip the same guard that catches actually depending on it.
    for (const { name, text } of rustSources) {
      expect(`${name}: ${stripComments(text)}`).not.toContain('serde_json');
    }
  });

  test('Cargo.toml declares no local path dependency', () => {
    // The other door out of the file scan above: a sibling crate need not
    // live anywhere under desktop/src-tauri at all. `brain = { path =
    // "../../somewhere/else" }` pulls in code the recursive scan above will
    // never see, no matter how wide its glob gets — Cargo.toml is the one
    // place that dependency has to be named, so it is the one place left
    // to check for it.
    expect(CARGO).not.toMatch(/path\s*=/);
  });

  test('the Rust knows no engine flag', () => {
    // The frontend builds the whole argv, --json included. A literal
    // starting with "--" in here would be the first piece of contract
    // knowledge to leak across the boundary.
    for (const { name, text } of rustSources) {
      const literals = text.match(/"(--[^"]*)"/g) ?? [];
      expect(`${name} contains flag literals: ${literals.join(', ')}`).toBe(
        `${name} contains flag literals: `,
      );
    }
  });

  test('the Rust knows nothing about screenplays or e-readers', () => {
    const banned = [
      'kindle', 'kobo', 'tolino', 'remarkable', 'calibre',
      'epub', 'mobi', 'azw3', 'kfx', 'fountain',
      'scene', 'screenplay', 'slug', 'dialogue', 'character',
      // Piece D: the shell forwards a diagnostic line without knowing that
      // one of them is progress, or that progress has a percent, or that
      // any of it is rendered. Those are the window's words, not Rust's.
      'progress', 'percent', 'render',
    ];
    for (const { name, text } of rustSources) {
      // Comments explaining the rule (main.rs's ADR summary, sidecar.rs's
      // module doc) are allowed to use the very words they're warning
      // about; only code is judged.
      const stripped = stripComments(text);
      // The file dialog's extension list is the one place a domain word
      // legitimately appears in code — it's an OS file filter, not a
      // decision about what a screenplay is. Carved out the same way the
      // "pdf" test below carves it out, rather than let it fail the ban.
      // The exemption removes the CALL, not the line: dropping a whole
      // line would let anything co-located on it escape the ban too.
      const lines = stripped.replace(/add_filter\([^)]*\)/g, '').split('\n');
      // Plain substring, not word-boundary: a `\b` match treats `_` as a
      // word character, so it is blind to snake_case and inflections
      // (`is_kindle_volume`, `n_scenes`, `sluglines`, `.epub3`) — exactly
      // the shape device/format/screenplay knowledge takes in real Rust.
      // The one real collision this creates — "epub" is the tail of the
      // product name "Screepub" and of the `screepub-engine` SIDECAR
      // constant — is carved out explicitly, by name, rather than by
      // loosening the match for every word.
      const lower = lines.join('\n').toLowerCase().replaceAll('screepub', '');
      for (const word of banned) {
        const found = lower.includes(word);
        expect(`${name} mentions ${word}: ${found}`).toBe(`${name} mentions ${word}: false`);
      }
    }
  });

  test('"pdf" appears only in the file dialog’s filter', () => {
    // The one domain word the Rust legitimately holds, because the OS file
    // picker needs an extension list and that is a window’s job. Anywhere
    // else it would mean the Rust had started deciding what a file is.
    for (const { name, text } of rustSources) {
      for (const line of text.split('\n')) {
        if (!line.toLowerCase().includes('pdf')) continue;
        expect(`${name}: ${line.trim()}`).toContain('add_filter');
      }
    }
  });

  test('exactly two commands are registered', () => {
    // A third command is the shape every "just one small thing in Rust"
    // takes. Adding one is allowed — but it must be a deliberate edit to
    // this list, in a diff someone reviews.
    const main = rustSources.find((f) => basename(f.name) === 'main.rs')!.text;
    const handler = main.match(/generate_handler!\[([^\]]*)\]/);
    expect(handler).not.toBeNull();
    const registered = handler![1].split(',').map((s) => s.trim()).filter(Boolean);
    expect(registered.sort()).toEqual(['pick_file', 'run_engine']);
  });

  test('the Rust ignores the exit code and answers with stdout', () => {
    // `--json` errors exit 1 while printing a valid error object. A shell
    // that failed on a non-zero status would turn every not-a-screenplay
    // into "the engine crashed", which is acceptance criterion 3 broken.
    //
    // Piece D replaced `.output()` with `.spawn()` so stderr could be
    // forwarded live, which is why this no longer looks for the literal
    // `output.stdout`. What it asserts instead is the RULE rather than the
    // spelling: there is exactly ONE way out with an error and it is the
    // empty-stdout case, the exit value is never compared to anything, and
    // the value handed to the window is the accumulated stdout.
    const sidecar = rustSources.find((f) => basename(f.name) === 'sidecar.rs')!.text;
    const body = stripComments(sidecar);

    // The literal the old assertion banned, still banned on RAW text so
    // this replacement is nowhere weaker than what it replaces.
    expect(sidecar).not.toMatch(/status\s*\.\s*success\s*\(\)/);
    // …and broadened: any `.success(` call at all, however it is spelled.
    expect(body).not.toMatch(/\.\s*success\s*\(/);

    // No equality or ordering test against the exit value, in either
    // operand order. Catches `status.code() != Some(0)`, `exit_code == 0`
    // and `Some(0) != exit` — each of which the narrower `\bcode\b`
    // spelling would sail straight past.
    expect(body).not.toMatch(/\b(status|code|exit)[\w.()]*\s*(==|!=|<=|>=)/i);
    expect(body).not.toMatch(/(==|!=|<=|>=)\s*[\w:.]*\b(status|code|exit)\b/i);
    // …and no branching on it by any other route.
    expect(body).not.toMatch(/\bmatch\s+[\w.()]*\b(status|code|exit)\b/i);

    // The structural half, which is what actually makes exit-code logic
    // impossible rather than merely awkward to spell: one `if` in the
    // file, testing stdout; one `Err(` construction, which is its body.
    const conditions = [...body.matchAll(/\bif\s+([^{]+)\{/g)].map((m) => m[1].trim());
    expect(conditions).toEqual(['stdout.is_empty()']);
    expect(body.match(/\bErr\(/g) ?? []).toHaveLength(1);

    // And the success path hands back the process's own stdout.
    expect(body).toMatch(/Stdout\(/);
    expect(body).toMatch(/Ok\(\s*stdout\s*\)/);

    // The assertions above are NOT airtight on their own, and it would be
    // worse than useless to pretend otherwise: a decision can be taken with
    // no `if` and no `Err(` at all —
    //
    //     exit.unwrap_or(0).eq(&0).then_some(()).ok_or_else(…)?;   // B
    //     assert_eq!(exit.unwrap_or(0), 0, "the engine failed");   // C
    //     let stdout = exit.filter(|c| *c == 0).map(…).unwrap_or_default();  // D
    //
    // — the last of which blanks stdout and lets the ONE legitimate `if`
    // report it, leaving both counts above untouched. Every such dodge was
    // written, compiled and run against this file; each passed everything
    // above.
    //
    // So the pin is a bare USE COUNT, because chasing method names
    // (`.eq`, `.filter`, `assert_eq!`, the next one nobody has thought of)
    // is a game the guard loses by construction. `exit` is allowed exactly
    // three appearances, and they are the only three it legitimately has:
    // it is BOUND, it is ASSIGNED from the terminated event, and it is
    // PRINTED inside the empty-stdout message. There is no fourth use of an
    // exit status that is not the Rust forming an opinion about it.
    //
    // If a refactor changes this number, that is the assertion doing its
    // job — work out which of the three moved, do not raise the count.
    expect(body.match(/\bexit\b/g) ?? []).toHaveLength(3);
    // A count reaches the binding it NAMES and not its source, so pin the
    // upstream one too: `status` can be inspected before it ever becomes
    // `exit`, leaving the count above untouched while the Rust decides a
    // non-zero exit means failure. Two uses: bound by the match arm, and
    // its code stored.
    expect(body.match(/\bstatus\b/g) ?? []).toHaveLength(2);
  });

  test('a forwarded line is forwarded, not read', () => {
    // The event carries a raw line. If the Rust ever learns what a line
    // MEANS — that it is progress, that it has a percent, that a stage is
    // named — the decision has moved out of TypeScript.
    const sidecar = rustSources.find((f) => basename(f.name) === 'sidecar.rs')!.text;
    const body = stripComments(sidecar);
    const lower = body.toLowerCase();
    for (const word of ['progress', 'percent', 'stage', 'ndjson', 'parse']) {
      expect(`sidecar.rs mentions ${word}: ${lower.includes(word)}`).toBe(
        `sidecar.rs mentions ${word}: false`,
      );
    }
    // Vocabulary alone is cheap to dodge: `line.starts_with("{\"p")` names
    // nothing on that list and is still the Rust deciding what a line is.
    // So ban every way of looking inside one. (`trim` stays allowed: it is
    // whitespace, and the stdout accumulator needs it.)
    for (const method of [
      'starts_with', 'ends_with', 'contains', 'split', 'strip_prefix',
      'strip_suffix', 'find(', 'char', 'bytes(', 'len()', 'replace',
    ]) {
      expect(`sidecar.rs calls ${method}: ${lower.includes(`.${method}`)}`).toBe(
        `sidecar.rs calls ${method}: false`,
      );
    }
    // The payload is the whole line binding — not a slice of it, not a
    // field picked out of it.
    expect(body).toMatch(/emit\(\s*LINE_EVENT\s*,\s*line(\s*\.\s*clone\(\))?\s*\)/);

    // …and, as above, the method-name list is a floor, not a fence:
    //
    //     let want = matches!(line.as_bytes().first(), Some(b'{'));  // F
    //     let want = line.get(0..1) == Some("{");                    // G
    //
    // dodge every name on it (`.as_bytes` is not `.bytes(`; `.get` and
    // index slicing are not named at all) and still decide what a line is.
    // Both were compiled and run; both passed the list.
    //
    // The count is therefore the real assertion. `line` may appear exactly
    // FOUR times, and all four are accounted for: once in the event-NAME
    // literal `"engine-line"`, then bound from the bytes, emitted, and
    // appended to the diagnostics buffer. A fifth is the Rust reading it.
    // Do not raise this number to make a refactor pass.
    expect(body.match(/(?<![A-Z_])\bline\b/g) ?? []).toHaveLength(4);
    // Same reasoning as `status` above: the raw `bytes` can be inspected
    // before they are ever decoded into `line`, which would filter WHICH
    // lines get forwarded while leaving the count above untouched. Four
    // uses: bound and decoded, once per stream.
    expect(body.match(/\bbytes\b/g) ?? []).toHaveLength(4);
  });
});

describe('the sidecar name is agreed on both sides', () => {
  test('the Rust asks for the basename the build tool writes', () => {
    // Two languages, one string. They cannot be checked by the compiler,
    // so they are checked here. A mismatch is a runtime "not found" in a
    // window — the exact failure Task 1 was arranged to prevent.
    const sidecar = rustSources.find((f) => basename(f.name) === 'sidecar.rs')!.text;
    expect(sidecar).toContain(`"${SIDECAR_BASENAME}"`);
  });

  test('tauri.conf.json points externalBin at the same basename', () => {
    expect(CONFIG.bundle.externalBin).toEqual([`binaries/${SIDECAR_BASENAME}`]);
  });
});

describe('the window is granted no more than it needs', () => {
  // This asserted `permissions` was exactly ['core:default'] until
  // 2026-09-21. It was a tripwire, and it fired as designed: it stopped the
  // first grant long enough for the decision to be made deliberately, in
  // ADR 2026-09-21 (doors, not commands), rather than in a diff.
  //
  // It is not loosened into "some permissions are fine". What replaces it is
  // narrower in the way that matters: every grant must be scoped, and the one
  // permission that would make the sidecar boundary meaningless is named and
  // forbidden outright.
  const capability = () => JSON.parse(
    readFileSync(join(REPO, 'desktop', 'src-tauri', 'capabilities', 'default.json'), 'utf8'),
  );

  test('the frontend can never spawn a process', () => {
    // The whole argument for the sidecar is that ONE binary is spawned and
    // the window chooses its arguments, not its identity. `shell:allow-execute`
    // hands the window the identity too, and then nothing about the boundary
    // is true. The ADR refuses it by name.
    const raw = JSON.stringify(capability());
    expect(raw).not.toContain('shell:allow-execute');
    expect(raw).not.toContain('shell:execute');
    // `shell:default` bundles execute, so the shorthand is refused as well.
    expect(raw).not.toContain('"shell:default"');
  });

  test('every plugin permission is scoped, never a bare grant', () => {
    // "The window is not granted 'open anything'; it is granted the bug
    // tracker, Amazon's two pages, and the library" — ADR 2026-09-21. A bare
    // string permission for a plugin is exactly the unscoped grant that
    // sentence refuses, so the shape is the test.
    for (const permission of capability().permissions) {
      if (typeof permission === 'string') {
        // Only core:* may be a bare string: it is the window's own baseline,
        // not a door onto the OS. Four named exceptions. updater:default has
        // no allow-list to write, because the plugin can only ever reach
        // the endpoints in tauri.conf.json. That list is its scope, and the
        // updater tests at the end of this file pin it to one URL on this
        // repository. process:allow-restart has nothing to scope at all: it
        // restarts this app and nothing else (tauri-plugin-process 2.3.1,
        // commands::restart is app.request_restart()). dialog:allow-open has
        // no allow-list either, and for a related reason: the user chooses
        // the folder themselves, in the OS's own picker, and the window only
        // receives the path it comes back with and hands it to the engine
        // (parity piece C, owner-approved 2026-09-23). dialog:allow-save
        // (parity piece B, owner-approved 2026-09-23) has nothing to scope
        // either: the user chooses the path in the native save box, and the
        // window only ever receives a path string back and hands it to the
        // engine, which is the one that writes the file.
        const allowed = permission.startsWith('core:')
          || permission === 'updater:default'
          || permission === 'process:allow-restart'
          || permission === 'dialog:allow-open'
          || permission === 'dialog:allow-save';
        expect(`bare permission: ${permission}`).toBe(`bare permission: ${
          allowed ? permission : `${permission} MUST BE SCOPED`}`);
        continue;
      }
      expect(Array.isArray(permission.allow)).toBe(true);
      expect(permission.allow.length).toBeGreaterThan(0);
    }
  });

  test('the folder picker needs no allow-list: the OS dialog is the scope', () => {
    // dialog:allow-open has nothing to narrow with an allow-list: the plugin
    // hands back whatever folder the user navigated to and picked in the
    // OS's own dialog, and the window only receives that path and gives it
    // to the engine (pickFolder, app.js). Owner-approved 2026-09-23.
    expect(capability().permissions).toContain('dialog:allow-open');
  });

  test('reveal is gone: the engine shows the file now, not the window', () => {
    // Owner decision, 2026-09-23: the window's reveal permission was fixed
    // to $DOCUMENT/Screepub, and once the library folder can move (piece
    // C's app-settings), a fixed-path door cannot follow it there. The
    // engine's own reveal verb does the showing now (ADR 2026-09-21's
    // amendment), so the window needs no reveal door at all, scoped or not.
    const raw = JSON.stringify(capability());
    expect(raw).not.toContain('opener:allow-reveal-item-in-dir');
  });

  test('the opener is never granted its default set, only the two scoped doors it needs', () => {
    // opener:default would grant every opener command (open-url, open-path,
    // reveal-item-in-dir) with no scope at all, exactly the "open anything"
    // the ADR refuses by name. Checked on the JSON text, the same pattern as
    // "the window may restart itself, and may not quit itself" above, so
    // the same identifier arriving as an object rather than a bare string
    // (`{identifier: "opener:default"}`) is caught too.
    const raw = JSON.stringify(capability().permissions);
    expect(raw).not.toMatch(/"opener:default"/);
  });

  test('the dialog plugin is never granted its default set, only the one folder-picker door it needs', () => {
    // dialog:default would grant every dialog command (open, save, message,
    // ask, confirm) with no scope at all. Piece B adds dialog:allow-save on
    // a parallel branch, merged in by hand later; this only refuses the
    // unscoped default, which leaves room for that second door to arrive
    // without this test needing to change.
    const raw = JSON.stringify(capability().permissions);
    expect(raw).not.toMatch(/"dialog:default"/);
  });

  test('the description matches the grants: no reveal clause for the window, still points at the ADR, no em dash', () => {
    const description = capability().description as string;
    // The old clause claimed the WINDOW could reveal a file; the window no
    // longer has that grant, so the sentence that promised it must be gone
    // too, not merely reworded around the same claim.
    expect(description).not.toContain('to reveal a file');
    expect(description).toContain('ADR 2026-09-21');
    expect(description).not.toContain(String.fromCharCode(0x2014));
  });

  test('the opener may reach the issue tracker and the KFX download pages, and nothing else', async () => {
    const { KFX_LINKS } = await import('../src/export/kfx-setup');
    const opener = capability().permissions.find(
      (p: unknown) => typeof p === 'object' && p !== null
        && (p as { identifier: string }).identifier === 'opener:allow-open-url',
    );
    expect(opener).toBeDefined();
    // The repository is a glob, so it must not widen past the repository:
    // "https://*" or a bare "https://github.com/*" would let any page on the
    // host be opened from whatever text the window happened to be holding.
    // The KFX pages are EXACT strings. tauri-plugin-opener 2.5.5 matches the
    // raw URL with glob::Pattern (src/scope.rs), where `*` also matches `/`,
    // so an exact string is the only grant that means one page.
    const exact = new Set<string>(KFX_LINKS);
    for (const entry of opener.allow) {
      const ok = entry.url.startsWith('https://github.com/ssandweiss/screepub/') || exact.has(entry.url);
      expect(`${entry.url} allowed: ${ok}`).toBe(`${entry.url} allowed: true`);
    }
    // And the other way round: every link the engine can put on the Send
    // page is granted, character for character. Without this half, a new
    // link in src/export/kfx-setup.ts would draw a button Tauri refuses.
    const granted = new Set(opener.allow.map((e: { url: string }) => e.url));
    for (const url of KFX_LINKS) {
      expect(`${url} granted: ${granted.has(url)}`).toBe(`${url} granted: true`);
    }
  });

  test('the identifier IS the Swift app’s, on purpose: that is the handover', () => {
    // This test used to assert the opposite, and was the tripwire that kept
    // the two apps from colliding while both shipped. It was inverted
    // deliberately at the identifier release (ADR 2026-09-20, plan
    // 2026-10-01-f2-handover-amendment.md): the installed Swift app's
    // updater pins com.darkwell.screepub, so sharing it is what lets that
    // updater install this app in its place. Do not change it back without
    // reading both.
    expect(CONFIG.identifier).toBe('com.darkwell.screepub');
  });

  test('the content security policy is the window’s own files, plus pictures written into the page', () => {
    // Pinned whole, because a policy is loosened one clause at a time and
    // every clause reads as harmless on its own.
    //
    // `default-src 'self'`: everything the window loads comes from its own
    // bundled files. It is also what refuses every inline script and style,
    // including a <style> inside the reader's srcdoc frame, and the window
    // works around that with constructed stylesheets (read.js, convert.js)
    // rather than by loosening it.
    //
    // `img-src 'self' data:`: the reader preview carries each picture inside
    // its `<img>` as a data: URI, because the preview is one document handed
    // to an iframe as text, with no folder for a path to point into. A srcdoc
    // frame inherits this policy, so without `data:` every figure in the
    // reader is a broken image. `'self'` is repeated because naming img-src
    // REPLACES default-src for pictures rather than adding to it: without it,
    // pictures would lose the window's own origin, which default-src gave
    // them. The window ships no image file today, so that half is about not
    // narrowing anything, not about a file that needs it.
    //
    // `connect-src`: the window's own origin and the device-request
    // service, the one address it posts to (tests/device-requests-ui.test.ts
    // pins it against the module that names it).
    //
    // Nothing else. No 'unsafe-inline', no blob:, and data: for pictures
    // only: a data: script or stylesheet is as much a way in as an inline one.
    // The whole security object, not just `csp`: a `devCsp`, `headers` or
    // `assetProtocol` key beside it could loosen the window without touching
    // the policy string.
    expect(CONFIG.app.security).toEqual({
      csp: "default-src 'self'; img-src 'self' data:; connect-src 'self' https://screepub-device-requests.samuelsandweiss.workers.dev",
    });
  });

  test('the frontend is static files, not a dev server', () => {
    // A `devUrl` or a `beforeDevCommand` would reintroduce the npm
    // toolchain this piece deliberately does without.
    expect(CONFIG.build.frontendDist).toBe('../ui');
    expect(CONFIG.build.devUrl).toBeUndefined();
    expect(CONFIG.build.beforeDevCommand).toBeUndefined();
  });
});

describe('the frontend owns the contract', () => {
  const APP_JS = readFileSync(join(REPO, 'desktop', 'ui', 'app.js'), 'utf8');

  test('it passes --json itself, because the Rust will not', () => {
    expect(APP_JS).toContain("'--json'");
  });

  test('it parses the engine’s answer', () => {
    expect(APP_JS).toContain('JSON.parse');
  });

  test('it invokes only the two commands that exist', () => {
    const names = [...APP_JS.matchAll(/invoke\(\s*'([a-z_]+)'/g)].map((m) => m[1]);
    expect([...new Set(names)].sort()).toEqual(['pick_file', 'run_engine']);
  });
});

describe('nothing outside desktop/ was drawn into the Rust shell', () => {
  test('no Rust file references the engine sources', () => {
    for (const { name, text } of rustSources) {
      expect(`${name} reaches outside desktop/: ${/\.\.\/\.\.\/src\b/.test(text)}`).toBe(
        `${name} reaches outside desktop/: false`,
      );
    }
  });
});

describe('the desktop CSS copy of the brand tokens does not drift', () => {
  // desktop/ui has no build step (see desktop/README.md) and can't @import
  // or copy brand/tokens.css at build time — tauri.conf.json's
  // frontendDist only embeds files under desktop/ui/, so a path outside it
  // 404s even in `cargo run`. style.css therefore inlines the seven core
  // colours literally, copied by hand from brand/tokens.json. A hand copy
  // is exactly the kind of value this repo already refuses to trust to a
  // human (see tests/brand-tokens.test.ts pinning tokens.css to
  // tokens.json, and tests/options.test.ts pinning format-defaults.json) —
  // so pin this copy the same way, against the one JSON source of truth.
  const CSS = readFileSync(join(REPO, 'desktop', 'ui', 'style.css'), 'utf8');
  const TOKENS = JSON.parse(readFileSync(join(REPO, 'brand', 'tokens.json'), 'utf8')).colors;

  // style.css declares its base palette in a top-level `:root { … }` block
  // and overrides a subset inside a `:root { … }` nested in
  // `@media (prefers-color-scheme: dark)`. The split below is keyed off
  // those actual `:root` blocks — not off "everything before/after the
  // @media marker" — so a hex custom property declared elsewhere in the
  // file (outside either block) is simply not swept into the wrong side.
  //
  // Known limits, same shape as tests/brand-tokens.test.ts's own note on
  // its block-scanning: this assumes exactly one top-level `:root {…}`
  // block and one `@media (prefers-color-scheme: dark) { :root {…} }`
  // block, both flat (no nested braces) and each on its own — a `}` inside
  // a comment or quoted value ahead of the real close would truncate the
  // slice early and silently.
  function rootBlock(text: string, searchFrom: number): string {
    const rootIdx = text.indexOf(':root', searchFrom);
    expect(rootIdx, `no :root block found at/after index ${searchFrom}`).toBeGreaterThan(-1);
    const braceStart = text.indexOf('{', rootIdx);
    const braceEnd = text.indexOf('}', braceStart);
    return text.slice(braceStart, braceEnd + 1);
  }

  const lightSection = rootBlock(CSS, 0);
  const mediaIdx = CSS.indexOf('@media (prefers-color-scheme: dark)');
  expect(mediaIdx, 'style.css is missing its dark-mode @media block').toBeGreaterThan(-1);
  const darkSection = rootBlock(CSS, mediaIdx);

  function hexDeclarations(section: string): Record<string, string> {
    const out: Record<string, string> = {};
    for (const [, name, value] of section.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{3,8})\s*;/g)) {
      out[name] = value;
    }
    return out;
  }

  const light = hexDeclarations(lightSection);
  const dark = hexDeclarations(darkSection);

  test('style.css declares at least the seven core tokens', () => {
    // Guards against the copy silently losing a value (e.g. a token
    // dropped during an edit) rather than only checking the ones present.
    expect(Object.keys(light).sort()).toEqual(
      ['alarm', 'brass', 'ground', 'ink', 'ink-muted', 'ink-on-brass', 'paper'].sort(),
    );
  });

  test('the dark override still declares its five tokens', () => {
    // Presence, not just value: deleting a dark override (e.g.
    // --ink-muted) would leave light-mode ink drawn on dark paper, and the
    // value-comparison test below can't catch an entry that's simply gone.
    expect(Object.keys(dark).sort()).toEqual(
      ['alarm', 'ground', 'ink', 'ink-muted', 'paper'].sort(),
    );
  });

  test('every light hex literal in style.css matches brand/tokens.json', () => {
    for (const [name, hex] of Object.entries(light)) {
      expect(TOKENS[name], `tokens.json has no color named ${name}`).toBeDefined();
      expect(hex.toLowerCase(), `style.css --${name} (light) drifted from tokens.json`).toBe(
        TOKENS[name].light.toLowerCase(),
      );
    }
  });

  test('every dark-mode hex literal in style.css matches brand/tokens.json', () => {
    for (const [name, hex] of Object.entries(dark)) {
      expect(TOKENS[name], `tokens.json has no color named ${name}`).toBeDefined();
      expect(hex.toLowerCase(), `style.css --${name} (dark) drifted from tokens.json`).toBe(
        TOKENS[name].dark.toLowerCase(),
      );
    }
  });
});

describe('the desktop workflow', () => {
  const WF = Bun.YAML.parse(
    readFileSync(join(REPO, '.github', 'workflows', 'desktop.yml'), 'utf8'),
  ) as {
    on: { push: { paths: string[] } };
    jobs: Record<string, { strategy?: { matrix?: { os?: string[] } }; steps: { run?: string }[] }>;
  };

  test('compiles on all three platforms', () => {
    // The spec's stated position is that macOS and Windows "ride on CI
    // building them". If this matrix quietly became ubuntu-only, that
    // sentence would be false and nothing else would notice.
    expect(WF.jobs.build.strategy?.matrix?.os).toEqual([
      'ubuntu-latest',
      'macos-15',
      'windows-latest',
    ]);
  });

  test('the sidecar is built before the shell is compiled', () => {
    // tauri-build needs it present. Reversed, every job fails with the
    // confusing not-found this piece exists to eliminate.
    const runs = WF.jobs.build.steps.map((s) => s.run ?? '');
    const sidecar = runs.findIndex((r) => r.includes('build-sidecar.ts'));
    const cargo = runs.findIndex((r) => r.includes('cargo build'));
    expect(sidecar).toBeGreaterThanOrEqual(0);
    expect(cargo).toBeGreaterThan(sidecar);
  });

  test('CI regenerates the window’s generated files rather than trusting them', () => {
    // Both are committed, so both can go stale between a brand/tokens.json
    // edit and someone noticing. Asserting the step exists is not enough:
    // assert it DIFFS, which is the half that makes it a gate.
    const runs = WF.jobs.build.steps.map((s) => s.run ?? '');
    const regen = runs.findIndex((r) => r.includes('build-desktop-tokens.ts'));
    expect(regen).toBeGreaterThanOrEqual(0);
    expect(runs[regen]).toContain('build-desktop-notes.ts');
    expect(runs[regen]).toContain('git diff --exit-code');
    const cargo = runs.findIndex((r) => r.includes('cargo build'));
    expect(regen).toBeLessThan(cargo);
  });

  test('it bundles, but signs nothing and never builds an AppImage', () => {
    // Was a scope guard against bundling at all, until piece E2 made
    // bundling on every push the point of the workflow. The two halves that
    // are still guards: no certificate touches the push path (signing
    // happens once, at a tag, in release.yml), and the AppImage bundler
    // corrupts the Bun-compiled sidecar, so its name must never appear.
    const all = JSON.stringify(WF);
    expect(all).toContain('cargo tauri build');
    expect(all).not.toContain('codesign');
    expect(all).not.toContain('APPLE_');
    expect(all).not.toContain('appimage');
  });

  test('it is path-filtered, so a parser change never waits on three cargo builds', () => {
    // Vacuous alternatives rejected: asserting the `on` key merely exists
    // would pass for a workflow that runs on every push, which is the thing
    // this test is for. Assert the filter itself.
    const triggers = (WF as unknown as { on: { push: { paths: string[] } } }).on;
    expect(triggers.push.paths).toContain('desktop/**');
    expect(triggers.push.paths).not.toContain('src/**');
  });
});

describe('the icon set the bundlers need', () => {
  const icons = join(REPO, 'desktop', 'src-tauri', 'icons');

  test('tauri.conf.json names every icon the three bundlers ask for', () => {
    // Order matters to nobody, presence matters to everybody: without
    // icons/icon.ico the Windows build script errors out before a single
    // Rust file compiles (tauri-build's lib.rs, "required for generating a
    // Windows Resource file"). desktop.yml's Windows leg has never run, so
    // this list is the only thing standing between it and a red first run.
    expect(CONFIG.bundle.icon).toEqual([
      'icons/32x32.png',
      'icons/128x128.png',
      'icons/128x128@2x.png',
      'icons/icon.png',
      'icons/icon.icns',
      'icons/icon.ico',
    ]);
  });

  test('every file it names is really there', () => {
    for (const rel of CONFIG.bundle.icon as string[]) {
      expect(existsSync(join(REPO, 'desktop', 'src-tauri', rel))).toBe(true);
    }
  });

  test('icon.ico is a real ICO and not a renamed PNG', () => {
    // A copied-and-renamed icon.png passes an existsSync check and then
    // fails the Windows build anyway. The ICONDIR header is 6 bytes:
    // reserved=0 (u16 LE), type=1 (u16 LE, 1 = icon), count > 0 (u16 LE).
    const head = readFileSync(join(icons, 'icon.ico')).subarray(0, 6);
    const u16 = (o: number) => head[o]! | (head[o + 1]! << 8);
    expect(u16(0)).toBe(0);
    expect(u16(2)).toBe(1);
    expect(u16(4)).toBeGreaterThan(0);
  });

  test('icon.icns is a real ICNS whose declared length matches the file', () => {
    // Same trap on the macOS side. The header is the ASCII magic 'icns'
    // followed by the total file length as a BIG-endian u32 -- so a
    // truncated copy fails here even though its first four bytes are right.
    const bytes = readFileSync(join(icons, 'icon.icns'));
    expect(bytes.subarray(0, 4).toString('latin1')).toBe('icns');
    expect(bytes.readUInt32BE(4)).toBe(bytes.length);
  });

  test('icon.png is still the 512-pixel square the Linux packages scale from', () => {
    // The .deb installs the largest PNG as the hicolor icon. IHDR puts
    // width and height at bytes 16..24, big-endian.
    const bytes = readFileSync(join(icons, 'icon.png'));
    expect(bytes.subarray(1, 4).toString('latin1')).toBe('PNG');
    expect(bytes.readUInt32BE(16)).toBe(512);
    expect(bytes.readUInt32BE(20)).toBe(512);
  });

  test('the platform icon sets nobody ships were not committed', () => {
    // `cargo tauri icon` also writes android/, ios/, Square*Logo.png and
    // StoreLogo.png. Screepub has no mobile build and no MSIX, so those are
    // 750 KB of files no bundler opens.
    for (const junk of ['android', 'ios', 'StoreLogo.png', 'Square44x44Logo.png']) {
      expect(existsSync(join(icons, junk))).toBe(false);
    }
  });
});

describe('what the Linux package tells a user about itself', () => {
  test('the publisher is the company, not a slice of the bundle identifier', () => {
    // Absent this, tauri-bundler derives Maintainer: from the SECOND segment
    // of the bundle identifier (then com.darkwell.screepub.desktop, now
    // com.darkwell.screepub; "darkwell" either way) and the .deb says
    // "Maintainer: darkwell". Observed, before this change, in a real build's
    // control file.
    expect(CONFIG.bundle.publisher).toBe('Darkwell Entertainment LLC');
  });

  test('the descriptions are written for a user, not for a contributor', () => {
    const short = CONFIG.bundle.shortDescription as string;
    const long = CONFIG.bundle.longDescription as string;
    // Both reach `apt show`. The crate's own description -- "A window around
    // the engine; no logic lives here" -- is a note to the next maintainer
    // and was what shipped.
    expect(short.length).toBeGreaterThan(20);
    expect(long.length).toBeGreaterThan(60);
    for (const text of [short, long]) {
      expect(text.toLowerCase()).not.toContain('sidecar');
      expect(text.toLowerCase()).not.toContain('shell');
      expect(text.toLowerCase()).not.toContain('no logic lives here');
    }
    // The short one also becomes Comment= in the .desktop entry, where a
    // trailing newline or a leading space would be copied verbatim.
    expect(short).toBe(short.trim());
    expect(short).not.toContain('\n');
  });

  test('the AGPL text and the third-party notices travel with the binary', () => {
    // The Swift app's build script put both inside its bundle for exactly
    // this reason: the AGPL requires the licence to accompany the work, and the
    // compiled engine embeds Apache-2.0 and MIT libraries. Paths are
    // relative to tauri.conf.json, hence ../../.
    expect(CONFIG.bundle.resources).toEqual({
      '../../LICENSE': 'LICENSE',
      '../../THIRD-PARTY-NOTICES.md': 'THIRD-PARTY-NOTICES.md',
    });
    // And the sources really exist, or the bundle step fails minutes later
    // with a glob that matched nothing.
    expect(existsSync(join(REPO, 'LICENSE'))).toBe(true);
    expect(existsSync(join(REPO, 'THIRD-PARTY-NOTICES.md'))).toBe(true);
  });

  test('the launcher files itself under Office and claims no file it cannot open', () => {
    // Category is real: it puts the app in the right menu.
    expect(CONFIG.bundle.category).toBe('Productivity');
    // fileAssociations is DELIBERATELY ABSENT. It was set, and task 6 found
    // it inert: the generated Exec= line carries no %f, and main.rs never
    // reads argv, so "Open with Screepub" put the app in the user's menu and
    // then opened an EMPTY WINDOW. Advertising a capability the app does not
    // have is worse than not appearing in the list, so the claim is withdrawn
    // until the shell can accept a path. Restoring this key without also
    // handling argv re-creates the empty window.
    expect(CONFIG.bundle.fileAssociations).toBeUndefined();
  });

  test('the window title is NOT changed by any of this', () => {
    // productName drives the package name and the .app filename; the window
    // title is a separate key. Task 10 overrides productName for macOS only,
    // and this is the assertion that catches the overlay reaching too far.
    expect(CONFIG.productName).toBe('Screepub');
    expect(CONFIG.app.windows[0].title).toBe('Screepub');
  });
});

describe('the Mac it claims to run on is the Mac it can run on', () => {
  test('the bundle asks for macOS 13 or later, the engine’s own floor', () => {
    // Without this the bundle says 10.13 (Tauri's default) while the engine
    // inside it is built for 13.0: measured 2026-10-01 with `otool -l` on an
    // installed window, `LC_BUILD_VERSION ... minos 13.0` on
    // Contents/MacOS/screepub-engine (bun's floor). On macOS 11 or 12 the
    // window then opens and every conversion fails. Saying 13.0 makes
    // Finder refuse up front, with a message naming the version. Raise this
    // when bun raises its floor; never lower it below the engine's minos.
    expect(CONFIG.bundle.macOS?.minimumSystemVersion).toBe('13.0');
  });
});

describe('the window has a floor', () => {
  test('it cannot be dragged smaller than 640 by 480', () => {
    // With no minimum the window could be squeezed until the drop well, the
    // tabs and the page margins overlapped. 640 by 480 still fits the narrow
    // layout (surfaces.css folds to one column well above it).
    const w = CONFIG.app.windows[0];
    expect([w.minWidth, w.minHeight]).toEqual([640, 480]);
    expect([w.width, w.height]).toEqual([1280, 800]);
  });
});

describe('one name on every platform: the transition overlay is gone', () => {
  // Until the identifier release the macOS build carried an overlay,
  // tauri.transition.conf.json, that renamed the app "Screepub Desktop" so it
  // could be installed beside the Swift app's Screepub.app. At the handover
  // the window BECOMES that app (same identifier, same name), so the overlay
  // was deleted, not edited. Plan 2026-10-01-f2-handover-amendment.md.
  const srcTauri = join(REPO, 'desktop', 'src-tauri');

  test('the overlay file does not exist', () => {
    expect(existsSync(join(srcTauri, 'tauri.transition.conf.json'))).toBe(false);
  });

  test('no workflow passes it, and no tool names it', () => {
    // A leftover `--config tauri.transition.conf.json` would fail the
    // release's macOS leg outright (the file is gone), which is at least
    // loud; this catches it before a tag does.
    const offenders: string[] = [];
    for (const dir of [join(REPO, '.github', 'workflows'), join(REPO, 'tools')]) {
      for (const f of readdirSync(dir)) {
        if (!/\.(ya?ml|ts|sh)$/.test(f)) continue;
        if (readFileSync(join(dir, f), 'utf8').includes('tauri.transition.conf.json')) offenders.push(f);
      }
    }
    expect(offenders).toEqual([]);
  });

  test('the only overlay left renames nothing and re-identifies nothing', () => {
    // tauri.updater.conf.json exists for its own reason (the updater flag
    // needs the signing key, so it cannot live in tauri.conf.json). It must
    // never become a second way to give the Mac build a different name.
    const overlays = readdirSync(srcTauri).filter((f) => /^tauri\..+\.conf\.json$/.test(f));
    expect(overlays).toEqual(['tauri.updater.conf.json']);
    const updater = JSON.parse(readFileSync(join(srcTauri, 'tauri.updater.conf.json'), 'utf8')) as Record<string, unknown>;
    expect(updater.productName).toBeUndefined();
    expect(updater.identifier).toBeUndefined();
  });

  test('the bundle and the window are both "Screepub"', () => {
    // The .app inside the Mac image is now Screepub.app, the same name the
    // Swift app's updater swaps into place (it takes the first .app it finds
    // in the image, whatever its name, and installs it at its own path).
    expect(CONFIG.productName).toBe('Screepub');
    expect(CONFIG.app.windows[0].title).toBe('Screepub');
  });
});

describe('the updater: transport in the crate, judgement in the engine', () => {
  // Piece A of docs/superpowers/plans/2026-09-21-parity.md. The plugin
  // moves bytes: it fetches latest.json, verifies a minisign signature,
  // downloads the archive and swaps the bundle. It decides nothing about
  // WHETHER to: that is src/update/compare.ts, reached from the window
  // through desktop/ui/update-compare.js. See
  // docs/superpowers/specs/2026-09-21-updater-design.md and the transport
  // plan beside it.
  const capability = () => JSON.parse(
    readFileSync(join(REPO, 'desktop', 'src-tauri', 'capabilities', 'default.json'), 'utf8'),
  );
  const LOCK = readFileSync(join(REPO, 'desktop', 'src-tauri', 'Cargo.lock'), 'utf8');

  test('the crate depends on the updater plugin, and the lockfile resolves the measured version', () => {
    // The platform-key rule, the error on a missing platform and the
    // macOS install path were all read off 2.12.0's source. The lockfile
    // is what pins that; the Cargo.toml line is the floor.
    expect(CARGO).toMatch(/^tauri-plugin-updater = "2\.12\.0"$/m);
    expect(LOCK).toMatch(/^name = "tauri-plugin-updater"\nversion = "2\.12\.0"$/m);
  });

  test('main.rs registers it, and registers no command for it', () => {
    const main = rustSources.find((f) => basename(f.name) === 'main.rs')!.text;
    const bare = stripComments(main);
    expect(bare).toContain('.plugin(tauri_plugin_updater::Builder::new().build())');
    // The plugin brings its own commands (check, download, install), which
    // the capability exposes. This crate adds none: the invoke handler
    // still names exactly the two commands the ADR allows.
    expect(bare).toMatch(/generate_handler!\[run_engine, pick_file\]/);
  });

  test('the process plugin is linked for one reason: restarting after an update', () => {
    // Approved by the owner 2026-09-23, after 0.7.2 installed itself and the
    // window kept running 0.7.1 with nothing on screen to say why.
    expect(CARGO).toMatch(/^tauri-plugin-process = "2\.3\.1"$/m);
    expect(LOCK).toMatch(/^name = "tauri-plugin-process"\nversion = "2\.3\.1"$/m);
    const main = rustSources.find((f) => basename(f.name) === 'main.rs')!.text;
    expect(stripComments(main)).toContain('.plugin(tauri_plugin_process::init())');
  });

  test('the window may restart itself, and may not quit itself', () => {
    // The plugin's default set grants exit AND restart. Only restart is
    // ours. Checked on the JSON text, not `.toContain` on the array itself:
    // an array check only catches a BARE string grant, and misses the same
    // identifier arriving as an object (e.g. `{identifier: "process:default"}`).
    const permissions = capability().permissions;
    expect(permissions).toContain('process:allow-restart');
    expect(JSON.stringify(permissions)).not.toMatch(/"process:(default|allow-exit)"/);
  });

  test('the window can be dragged, and zooming it needed nothing new', () => {
    // start_dragging is NOT in core:default; internal_toggle_maximize IS
    // (tauri 2.11.5 permissions/window/autogenerated/reference.md), so the
    // double-click zoom needs no grant of its own.
    const permissions = capability().permissions;
    expect(permissions).toContain('core:window:allow-start-dragging');
    expect(permissions).not.toContain('core:window:allow-internal-toggle-maximize');
  });

  test('the window may show a save dialog, and that grant is bare because there is nothing to scope', () => {
    // Parity piece B, owner-approved 2026-09-23 ("yes go for it"). The window
    // gets exactly the save half of the dialog plugin: pick_file stays a Rust
    // command (it already was), and the JS side gains only dialog:allow-save,
    // never dialog:default, which would also hand it message/ask/confirm it
    // has no use for. dialog:allow-open is the one other half it holds: piece
    // C's folder picker for where books are saved, pinned in its own test.
    const permissions = capability().permissions;
    expect(permissions).toContain('dialog:allow-save');
    expect(permissions).not.toContain('dialog:default');
    expect(JSON.stringify(permissions)).not.toMatch(/"dialog:allow-(message|ask|confirm)"/);
  });

  test('the window may check, download and install, and that grant is bare BY NECESSITY', () => {
    // updater:default is check + download + install + download-and-install,
    // read off the plugin's permissions/default.toml. It has no allow-list
    // to scope, because the plugin can only ever reach the endpoints in
    // tauri.conf.json, which the next test pins. So this is the one plugin
    // permission the scoped-grant rule names as an exception.
    expect(capability().permissions).toContain('updater:default');
  });

  test('the endpoint is exactly one URL, on this repository’s releases, over TLS', () => {
    // releases/latest/download/<asset> is GitHub's redirect to the newest
    // non-prerelease, non-draft release's asset. One URL, not a list: a
    // second endpoint is a second place for the manifest to go stale.
    const updater = CONFIG.plugins?.updater;
    expect(updater).toBeDefined();
    expect(updater.endpoints).toEqual([
      'https://github.com/ssandweiss/screepub/releases/latest/download/latest.json',
    ]);
  });

  test('the public key is either empty (not yet supplied) or a real minisign public key', () => {
    // EMPTY is a working-branch state: the owner generates the key pair
    // and hands over the public half (docs/release-secrets.md §4). A tag
    // with an empty key is refused by release.yml's checks job, not by
    // this test, because a test that fails until a person acts is a red
    // main for nobody's fault. What this test refuses is GARBAGE: a key
    // that is set and is not a minisign public key box would make the
    // updater refuse every release, silently, forever.
    const pubkey = CONFIG.plugins.updater.pubkey;
    expect(typeof pubkey).toBe('string');
    if (pubkey === '') return;
    const decoded = Buffer.from(pubkey, 'base64').toString('utf8');
    const lines = decoded.trim().split('\n');
    // `tauri signer generate` base64-encodes the whole minisign box: a
    // comment line carrying the key id, then the 56-character key.
    expect(lines[0]).toMatch(/^untrusted comment: minisign public key: [0-9A-Fa-f]{16}$/);
    expect(lines[1]).toMatch(/^RW[A-Za-z0-9+/]{54}$/);
  });

  test('the plugin’s JavaScript reaches the window without a build step', () => {
    // The plugin's build.rs registers its api-iife.js as a global API
    // script, which Tauri injects as window.__TAURI__.updater ONLY when
    // withGlobalTauri is on. desktop/ui has no bundler to import
    // @tauri-apps/plugin-updater from, so this flag is the whole bridge.
    expect(CONFIG.app.withGlobalTauri).toBe(true);
  });

  test('no update decision leaks into the crate', () => {
    // The plugin compares versions with semver internally and the window
    // overrules it with the engine's comparator. Nothing in this crate may
    // add a third opinion.
    for (const { name, text } of rustSources) {
      const bare = stripComments(text);
      expect(`${name} decides versions: ${/\bsemver\b|Version::parse|is_newer|isNewer/.test(bare)}`)
        .toBe(`${name} decides versions: false`);
    }
  });
});

describe('the updater overlay: the archive is a RELEASE artifact, not a build artifact', () => {
  // tauri-cli signs updater artifacts itself, whenever
  // bundle.createUpdaterArtifacts is on, and fails the whole bundle with "A
  // public key has been found, but no private key" when
  // TAURI_SIGNING_PRIVATE_KEY is unset (src/bundle.rs, sign_updaters). So
  // the flag cannot live in tauri.conf.json: desktop.yml bundles on every
  // push with no secrets, and so does anyone running cargo tauri build at
  // home. It lives in an overlay that only release.yml's macOS leg passes,
  // through build-app-bundle.ts --updater.
  const overlayPath = join(REPO, 'desktop', 'src-tauri', 'tauri.updater.conf.json');
  const overlay = () => JSON.parse(readFileSync(overlayPath, 'utf8')) as Record<string, unknown>;

  /** tauri-cli's merge (helpers/config.rs, merge_patches): RFC 7396,
   *  descending into objects. NOT the shallow spread the transition
   *  overlay's test uses, because this overlay reaches INSIDE `bundle`
   *  and a shallow merge would replace the whole table. */
  function mergePatch(doc: unknown, patch: unknown): unknown {
    if (typeof patch !== 'object' || patch === null || Array.isArray(patch)) return patch;
    const out: Record<string, unknown> =
      typeof doc === 'object' && doc !== null && !Array.isArray(doc)
        ? { ...(doc as Record<string, unknown>) }
        : {};
    for (const [k, v] of Object.entries(patch as Record<string, unknown>)) {
      out[k] = mergePatch(out[k], v);
    }
    return out;
  }

  test('it exists and turns on exactly the one flag', () => {
    expect(existsSync(overlayPath)).toBe(true);
    expect(overlay()).toEqual({ bundle: { createUpdaterArtifacts: true } });
  });

  test('merged the way the CLI merges, it changes that flag and nothing else', () => {
    const merged = mergePatch(CONFIG, overlay()) as typeof CONFIG;
    expect(merged.bundle.createUpdaterArtifacts).toBe(true);
    for (const key of Object.keys(CONFIG.bundle)) {
      expect(merged.bundle[key]).toEqual(CONFIG.bundle[key]);
    }
    for (const key of Object.keys(CONFIG)) {
      if (key === 'bundle') continue;
      expect(merged[key]).toEqual(CONFIG[key]);
    }
  });

  test('tauri.conf.json itself never turns the archive on', () => {
    expect(CONFIG.bundle.createUpdaterArtifacts).toBeUndefined();
  });

  test('the push workflow never passes it', () => {
    // No secret reaches desktop.yml, by design (its own comment). Passing
    // this overlay there would fail every push at the bundle step.
    const desktopYml = readFileSync(join(REPO, '.github', 'workflows', 'desktop.yml'), 'utf8');
    expect(desktopYml).not.toContain('tauri.updater.conf.json');
    expect(desktopYml).not.toContain('--updater');
  });

});

describe('what sidecar.rs says about a quit mid-call is true of the engine', () => {
  // sidecar.rs lets an engine call finish when the app quits under it, and
  // says why that is safe: which of the engine's writes go through a
  // temporary file and a rename, and which are made in place. That is a
  // claim about src/, so this counts the engine's file writes by file. A new
  // write, or one that moved, fails here until someone has checked whether
  // the comment is still true and changed the count to match.
  const engineWrites = () => {
    const found: Record<string, number> = {};
    const src = join(REPO, 'src');
    for (const f of readdirSync(src, { recursive: true }) as string[]) {
      if (!f.endsWith('.ts')) continue;
      const calls = readFileSync(join(src, f), 'utf8')
        .match(/\b(writeFileSync|writeFile|copyFileSync|copyFile|appendFileSync|appendFile|createWriteStream|Bun\.write)\(/g);
      if (calls !== null) found[`src/${f.split('\\').join('/')}`] = calls.length;
    }
    return found;
  };

  test('every engine write is temp-then-rename except the ones the comment names, and the decoder copy into a private temp folder', () => {
    expect(engineWrites()).toEqual({
      // temp-then-rename: the writeFileAtomic helpers and their like
      'src/cli.ts': 2, // writeFileAtomic, and the --debug dump written in place
      'src/cli-export.ts': 1,
      // writeFileAtomicSync and copyFileAtomicSync, shared by the settings
      // files, the Kindle artifacts and the library's source.json marker and
      // adopted sidecar
      'src/atomic-write.ts': 2,
      'src/replace-file.ts': 1,
      // In place, and safe to be: pdf.js's image decoders, copied into a
      // private folder the engine makes fresh in the system temp folder and
      // removes at exit. Nothing outside that one process ever reads it, so
      // even a kill mid-copy loses nothing of the user's, and the comment,
      // which names what a person would miss, has nothing to add.
      'src/parser/pdfjs-wasm.ts': 1,
    });
    const sidecar = rustSources.find((f) => basename(f.name) === 'sidecar.rs')!.text;
    const comment = sidecar.slice(sidecar.indexOf('pub async fn run('), sidecar.indexOf('let (mut rx'));
    for (const named of ['src/library.ts', 'source.json', 'sidecar', 'debug', 'temporary file']) {
      expect(`sidecar.rs's quit comment names ${named}: ${comment.includes(named)}`)
        .toBe(`sidecar.rs's quit comment names ${named}: true`);
    }
  });
});
