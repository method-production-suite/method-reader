# Page-Span Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill pages on keep-honoring renderers (KFX, Apple Books) by shrinking the scene-heading keep to a CSS chain, adding the transition never-begins-a-page rule, falling tall dual-dialogue exchanges back to sequential speeches, and adding an opt-in `keepSpeechesWhole` option that makes whole speeches unbreakable.

**Architecture:** One new boolean flows through the single knob surface (engine `FormatOptions` ↔ root `format-defaults.json` ↔ app `FormatSettings`); CSS emission in `src/epub/css.ts` becomes conditional in two places; `src/epub/html.ts` loses the heading wrapper and gains a size-estimated dual-dialogue fallback. Defaults are pinned three ways (`tests/options.test.ts`, kit-check, `format-defaults.json`) — every default-surface edit moves all pinned files in the same commit.

**Tech Stack:** Bun/TypeScript engine (`bun test`, `bunx tsc --noEmit`), SwiftPM app checked by the `kit-check` executable (`cd app && swift run -c release kit-check`), fountain-js tokens → XHTML/CSS.

**Branch:** `page-span-rules` (cut from main). Spec: `docs/superpowers/specs/2026-07-30-page-span-rules-design.md` on branch `worktree-device-map` (not present on this branch — this plan is self-contained).

**Run all engine commands from the repo root** (the worktree root containing `package.json`).

---

### Task 1: `keepSpeechesWhole` option in the engine knob surface

**Files:**
- Modify: `src/options.ts` (interface ~line 17, defaults ~line 49, resolver ~line 84)
- Modify: `format-defaults.json`
- Test: `tests/options.test.ts` (append a describe at end of file)

- [ ] **Step 1: Write the failing test.** Append at the end of `tests/options.test.ts` (all imports it needs are already at the top of the file):

```ts
// ── keepSpeechesWhole option ─────────────────────────────

describe('keepSpeechesWhole option', () => {
  test('defaults off and resolves from partials', () => {
    expect(resolveFormatOptions({}).keepSpeechesWhole).toBe(false);
    expect(resolveFormatOptions({ keepSpeechesWhole: true }).keepSpeechesWhole).toBe(true);
    expect(resolveFormatOptions({ keepSpeechesWhole: 'yes' }).keepSpeechesWhole).toBe(false);
  });
});
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `bun test tests/options.test.ts`
Expected: the new test FAILS (`expect(undefined).toBe(false)` — the property does not exist yet). All pre-existing tests still pass.

- [ ] **Step 3: Implement.** Three edits, one commit — the pinning test couples them.

In `src/options.ts`, after the `keepSceneHeadingWithScene: boolean;` interface line add:

```ts
  /** whole speeches ride one page: dialogue blocks become unbreakable;
   * off = the proven default, cue + first line keep only (registry #8c) */
  keepSpeechesWhole: boolean;
```

In `DEFAULT_FORMAT_OPTIONS`, after `keepSceneHeadingWithScene: true,` add:

```ts
  keepSpeechesWhole: false,
```

In `resolveFormatOptions`'s return object, after `keepSceneHeadingWithScene: bool('keepSceneHeadingWithScene'),` add:

```ts
    keepSpeechesWhole: bool('keepSpeechesWhole'),
```

In `format-defaults.json`, after the `"keepSceneHeadingWithScene": true,` line add:

```json
  "keepSpeechesWhole": false,
```

- [ ] **Step 4: Verify green.**

Run: `bun test tests/options.test.ts && bunx tsc --noEmit`
Expected: PASS (including the canonical-file pinning test — it reads `format-defaults.json` and compares against the defaults, which moved together).

- [ ] **Step 5: Commit.**

```bash
git add src/options.ts format-defaults.json tests/options.test.ts
git commit -m "keepSpeechesWhole joins the knob surface, default off"
```

---

### Task 2: CSS — atomic dialogue blocks when the option is on

**Files:**
- Modify: `src/epub/css.ts` (`screenplayCss`, `.dialogue-block` rule ~line 99)
- Test: `tests/options.test.ts` (inside the existing CSS-options describe that contains the `scene page breaks add a break rule only when enabled` test)

- [ ] **Step 1: Write the failing test.** Add beside the `scene page breaks` test:

```ts
  test('keepSpeechesWhole makes dialogue blocks unbreakable only when enabled', () => {
    const off = screenplayCss(resolveFormatOptions({}));
    expect(off.match(/\.dialogue-block\s*{[^}]*}/)![0]).not.toContain('break-inside');
    const on = screenplayCss(resolveFormatOptions({ keepSpeechesWhole: true }));
    const block = on.match(/\.dialogue-block\s*{[^}]*}/)![0];
    expect(block).toContain('page-break-inside: avoid');
    expect(block).toContain('break-inside: avoid');
  });
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `bun test tests/options.test.ts`
Expected: FAIL on the `on` assertions (rule lacks the avoid lines).

- [ ] **Step 3: Implement.** In `src/epub/css.ts` inside `screenplayCss`, after the `bodyAlign` const add:

```ts
  // Atomic speeches (registry #8c): opt-in, because an unbreakable block
  // taller than the space left on a page gets pushed whole — bounded
  // white space traded for never splitting a speech.
  const speechKeep = o.keepSpeechesWhole
    ? '\n  page-break-inside: avoid;\n  break-inside: avoid;'
    : '';
```

and change the `.dialogue-block` rule's last margin line to interpolate it:

```
.dialogue-block {
  margin-top: ${em(gap)};
  margin-bottom: ${em(gap)};
  margin-left: ${o.dialogueSideMarginPct}%;
  margin-right: ${o.dialogueSideMarginPct}%;${speechKeep}
}
```

- [ ] **Step 4: Verify green.**

Run: `bun test tests/options.test.ts tests/epub.test.ts`
Expected: PASS (default CSS unchanged, so no epub.test.ts assertion moves).

- [ ] **Step 5: Commit.**

```bash
git add src/epub/css.ts tests/options.test.ts
git commit -m "Atomic dialogue blocks under keepSpeechesWhole"
```

---

### Task 3: Transitions may end a page, never begin one

**Files:**
- Modify: `src/epub/css.ts` (`p.transition` rule ~line 144)
- Test: `tests/epub.test.ts` (inside the `screenplay CSS (Kindle-safe geometry)` describe)

- [ ] **Step 1: Write the failing test.**

```ts
  test('transitions may end a page but never begin one', () => {
    const t = SCREENPLAY_CSS.match(/p\.transition\s*{[^}]*}/)![0];
    expect(t).toContain('page-break-before: avoid');
    expect(t).toContain('break-before: avoid');
  });
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `bun test tests/epub.test.ts`
Expected: FAIL (rule has no break properties).

- [ ] **Step 3: Implement.** The `p.transition` rule becomes:

```
/* A transition belongs to the shot before it: it may end a page, never
   begin one (the universal print rule, docs/pagination-reference.md). */
p.transition {
  text-align: right;
  text-transform: uppercase;
  margin: ${em(gap)} 0;
  page-break-before: avoid;
  break-before: avoid;
}
```

- [ ] **Step 4: Verify green.** Run: `bun test tests/epub.test.ts` — PASS.

- [ ] **Step 5: Commit.**

```bash
git add src/epub/css.ts tests/epub.test.ts
git commit -m "Transitions never begin a page"
```

---

### Task 4: Heading keep shrinks from wrapper to CSS chain

**Files:**
- Modify: `src/epub/html.ts` (delete `renderScene` ~lines 204-223; its call site in `tokensToBody` ~line 269)
- Modify: `src/epub/css.ts` (h2 rule ~line 51 gains the option gate; `.keep-together` comment ~line 43)
- Test: `tests/epub.test.ts` (rewrite the three wrapper tests at ~lines 57-80), `tests/options.test.ts` (rewrite the toggle test at ~line 114)

- [ ] **Step 1: Rewrite the tests to the new contract (failing first).**

In `tests/epub.test.ts`, REPLACE the test `scene heading + first block are wrapped to keep together across page breaks` with:

```ts
  test('scene heading leads its scene bare — the keep is the CSS chain, not a wrapper', () => {
    const [file] = tokensToBody(sampleTokens()).files;
    expect(file.xhtml).toMatch(
      /<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. KITCHEN - DAY<\/h2>\s*<p class="action">Jack enters, exhausted\.<\/p>/,
    );
    expect(file.xhtml).not.toMatch(/<div class="keep-together">\s*<h2/);
  });
```

REPLACE `a dialogue block directly after a heading keeps together as one unit` with:

```ts
  test('a dialogue block follows its heading directly — no outer wrapper', () => {
    const tokens = new Fountain().parse('INT. CAR - DAY\n\n@DEV\nDrive.\n\nThey drive.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<h2 class="scene-heading">INT\. CAR - DAY<\/h2>\s*<div class="dialogue-block">/,
    );
    expect(file.xhtml).not.toMatch(/<div class="keep-together">\s*<h2/);
  });
```

REPLACE `heading-only scene wraps without error` with:

```ts
  test('heading-only scene renders bare without error', () => {
    const tokens = new Fountain().parse('INT. VOID - DAY\n\nEXT. VOID - NIGHT\n\nStars.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. VOID - DAY<\/h2>\s*<\/section>/,
    );
  });
```

In `tests/options.test.ts`, REPLACE the test `scene-heading keep-together wrapper can be disabled` with:

```ts
  test('keepSceneHeadingWithScene gates the heading break-after rule', () => {
    const on = screenplayCss(resolveFormatOptions({}));
    expect(on.match(/h2\.scene-heading\s*{[^}]*}/)![0]).toContain('break-after: avoid');
    const off = screenplayCss(resolveFormatOptions({ keepSceneHeadingWithScene: false }));
    expect(off.match(/h2\.scene-heading\s*{[^}]*}/)![0]).not.toContain('break-after');
    // markup carries no wrapper either way
    const body = tokensToBody(tokens(), { format: resolveFormatOptions({}) });
    expect(body.files[0].xhtml).not.toMatch(/<div class="keep-together">\s*<h2/);
  });
```

- [ ] **Step 2: Run to verify the new tests fail.**

Run: `bun test tests/epub.test.ts tests/options.test.ts`
Expected: the three rewritten epub tests FAIL (wrapper still present); the options test FAILS (`off` CSS still contains `break-after`).

- [ ] **Step 3: Implement.**

In `src/epub/html.ts`: delete the whole `renderScene` function (with its doc comment), and in `tokensToBody` change the section-building line from

```ts
    const html = `<section class="scene" id="${anchor}">\n${renderScene(g.tokens, startsWithHeading, format, markers)}</section>\n`;
```

to

```ts
    const html = `<section class="scene" id="${anchor}">\n${renderBlocks(g.tokens, format, markers).join('')}</section>\n`;
```

and delete the now-unused `const startsWithHeading = g.tokens[0]?.type === 'scene_heading';` line.

In `src/epub/css.ts` inside `screenplayCss`, add beside `speechKeep`:

```ts
  // The heading keep is a CHAIN, not a wrapper: break-after on the h2
  // holds it to whatever follows, without making the whole first block
  // unbreakable (the old wrapper pushed half-page chunks; registry #5a).
  const headingKeep = o.keepSceneHeadingWithScene
    ? '\n  page-break-after: avoid;\n  break-after: avoid;'
    : '';
```

and change the `h2.scene-heading` rule to:

```
h2.scene-heading {
  font-size: 1em;
  font-weight: bold;
  text-transform: uppercase;
  margin: ${em(gap * 1.6)} 0 ${em(gap)} 0;${headingKeep}
}
```

Also update the `.keep-together` comment block (the rule itself stays — `closeSpeech` still uses the class):

```
/* The cue keep: cue + parentheticals + first dialogue line share this
   unbreakable wrapper so a cue never strands at a page bottom.
   Container-level inside-avoid is the KDP-documented keep-together form. */
```

And in the css.ts file-header comment, update the line `// - Keep-with-next chain kept minimal (heading, cue) — every avoid link` — it stays accurate, no change needed.

- [ ] **Step 4: Run the full engine suite.**

Run: `bun test && bunx tsc --noEmit`
Expected: ALL PASS. Watch specifically: `keep-with-next chain is minimal` (still passes — default CSS keeps the h2 avoid), `opening content without a heading` (unchanged), the cue-keep tests around line 252 (untouched path).

- [ ] **Step 5: Commit.**

```bash
git add src/epub/html.ts src/epub/css.ts tests/epub.test.ts tests/options.test.ts
git commit -m "Heading keep becomes the CSS chain: wrapper gone, pages fill"
```

---

### Task 5: Tall dual-dialogue exchanges fall back to sequential

**Files:**
- Modify: `src/epub/html.ts` (`renderBlocks`: dual state ~line 94, `closeSpeech` ~line 112, `dialogue_end` ~line 150, `dual_dialogue_end` ~line 138; two new module constants)
- Test: `tests/epub.test.ts` (new describe after the `tokensToBody` describe)

- [ ] **Step 1: Write the failing tests.**

```ts
// ── dual dialogue height fallback ────────────────────────

describe('dual dialogue height fallback', () => {
  const dualScript = (line: string) =>
    `INT. HALL - DAY\n\n@JACK\n${line}\n\n@JILL ^\nAlso talking here.\n`;

  test('a short exchange stays a side-by-side table', () => {
    const tokens = new Fountain().parse(dualScript('Quick word.'), true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('<table class="dual-dialogue">');
  });

  test('a tall exchange emits sequential speeches instead of an unbreakable table', () => {
    const tokens = new Fountain().parse(dualScript('word '.repeat(120).trim()), true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).not.toContain('<table class="dual-dialogue">');
    expect(file.xhtml).toContain('<p class="character">JACK</p>');
    expect(file.xhtml).toContain('<p class="character">JILL</p>');
    // both speeches render as ordinary dialogue blocks, each with the cue keep
    const blocks = file.xhtml.match(/<div class="dialogue-block">/g) ?? [];
    expect(blocks.length).toBe(2);
  });
});
```

- [ ] **Step 2: Run to verify the tall case fails.**

Run: `bun test tests/epub.test.ts`
Expected: `a short exchange…` PASSES already; `a tall exchange…` FAILS (table present).

- [ ] **Step 3: Implement.** In `src/epub/html.ts`:

Add module constants after `DEFAULT_MAX_FILE_BYTES`:

```ts
/** A dual exchange whose taller column exceeds this many estimated
 * rendered lines cannot fit beside itself on one page; the unbreakable
 * table would push whole and leave a page-sized gap, so it degrades to
 * sequential speeches. Estimate assumes ~30 chars per half-width line. */
const DUAL_SEQUENTIAL_LINE_THRESHOLD = 12;
const EST_CHARS_PER_DUAL_LINE = 30;
```

Change the dual state to carry kinds (needed so the fallback can reuse the cue keep):

```ts
  let dual: {
    left: { kind: string; html: string }[];
    right: { kind: string; html: string }[];
    side: 'left' | 'right';
  } | null = null;
```

Factor the speech wrapper out of `closeSpeech` so the fallback shares it — replace the whole `closeSpeech` const with:

```ts
  const speechBlock = (cells: { kind: string; html: string }[]): string => {
    const firstLine = cells.findIndex((c) => c.kind === 'dialogue');
    const cut = firstLine === -1 ? cells.length : firstLine + 1;
    const head = cells.slice(0, cut).map((c) => c.html).join('');
    const tail = cells.slice(cut).map((c) => c.html).join('');
    return `<div class="dialogue-block">\n<div class="keep-together">\n${head}</div>\n${tail}</div>\n`;
  };
  const closeSpeech = () => {
    if (!speech) return;
    blocks.push(speechBlock(speech));
    speech = null;
  };
```

In the `dialogue_end` case, the dual branch becomes (objects, not `.map((c) => c.html)`):

```ts
      case 'dialogue_end':
        if (dual && speech) {
          dual[dual.side].push(...speech);
          speech = null;
        } else {
          closeSpeech();
        }
        break;
```

And `dual_dialogue_end` becomes:

```ts
      case 'dual_dialogue_end':
        if (dual) {
          const estLines = (col: { html: string }[]) =>
            col.reduce((n, c) => {
              const len = c.html.replace(/<[^>]*>/g, '').length;
              return n + Math.max(1, Math.ceil(len / EST_CHARS_PER_DUAL_LINE));
            }, 0);
          if (Math.max(estLines(dual.left), estLines(dual.right)) > DUAL_SEQUENTIAL_LINE_THRESHOLD) {
            if (dual.left.length) blocks.push(speechBlock(dual.left));
            if (dual.right.length) blocks.push(speechBlock(dual.right));
          } else {
            blocks.push(
              `<table class="dual-dialogue">\n<tr>\n<td>\n${dual.left.map((c) => c.html).join('')}</td>\n<td>\n${dual.right.map((c) => c.html).join('')}</td>\n</tr>\n</table>\n`,
            );
          }
          dual = null;
        }
        break;
```

- [ ] **Step 4: Run the full engine suite.**

Run: `bun test && bunx tsc --noEmit`
Expected: ALL PASS, including the existing `dual-dialogue table keep-together` CSS test (short exchanges unchanged).

- [ ] **Step 5: Commit.**

```bash
git add src/epub/html.ts tests/epub.test.ts
git commit -m "Tall dual exchanges degrade to sequential speeches"
```

---

### Task 6: Swift mirror — FormatSettings, sidecar, app keys, settings toggle

**Files:**
- Modify: `app/Sources/ScreepubKit/FormatSettings.swift` (field + defaults + init)
- Modify: `app/Sources/ScreepubKit/ScriptSettings.swift` (`PartialFormatSettings` + merge)
- Modify: `app/Sources/ScreepubApp/AppSettings.swift` (`formatSettings()`, `setFormatSettings`, `resetFormatting`)
- Modify: `app/Sources/ScreepubApp/ScreepubApp.swift` (`FormattingSettings`: `@AppStorage` + toggle in the Pages section)
- Test: `app/Sources/KitCheck/main.swift`

- [ ] **Step 1: Write the failing check.** In `app/Sources/KitCheck/main.swift`, directly after the format-defaults block (the check `FormatSettings.defaults matches the canonical file the engine pins`), add:

```swift
check(FormatSettings.defaults.keepSpeechesWhole == false,
      "keepSpeechesWhole defaults off — speeches flow; atomicity is opt-in")
```

- [ ] **Step 2: Verify it fails to compile.**

Run: `cd app && swift run -c release kit-check`
Expected: compile error — `value of type 'FormatSettings' has no member 'keepSpeechesWhole'`.

- [ ] **Step 3: Implement, in this order.**

`FormatSettings.swift` — after `public var keepSceneHeadingWithScene: Bool` add `public var keepSpeechesWhole: Bool`; in `defaults` after `keepSceneHeadingWithScene: true,` add `keepSpeechesWhole: false,`; in `init` add the parameter `keepSpeechesWhole: Bool,` after `keepSceneHeadingWithScene: Bool,` and the assignment `self.keepSpeechesWhole = keepSpeechesWhole` in position.

`ScriptSettings.swift` — in `PartialFormatSettings` after `var keepSceneHeadingWithScene: Bool?` add `var keepSpeechesWhole: Bool?`; in `load`'s merge after the `keepSceneHeadingWithScene` line add:

```swift
        if let v = partial.keepSpeechesWhole { merged.keepSpeechesWhole = v }
```

`AppSettings.swift` — in `formatSettings()` after `keepSceneHeadingWithScene: bool("fmtKeepHeading", def.keepSceneHeadingWithScene),` add:

```swift
            keepSpeechesWhole: bool("fmtKeepSpeeches", def.keepSpeechesWhole),
```

In `setFormatSettings` after the `fmtKeepHeading` line add:

```swift
        d.set(s.keepSpeechesWhole, forKey: "fmtKeepSpeeches")
```

In `resetFormatting`'s key array add `"fmtKeepSpeeches"` after `"fmtKeepHeading"`.

`ScreepubApp.swift` — in `FormattingSettings` after the `fmtKeepHeading` `@AppStorage` line add:

```swift
    @AppStorage("fmtKeepSpeeches") private var keepSpeeches = FormatSettings.defaults.keepSpeechesWhole
```

and in `Section("Pages")` after the `Keep scene headings with their scene` toggle add:

```swift
                Toggle("Keep each speech on one page", isOn: $keepSpeeches)
                Text("Avoids mid-speech page turns; long speeches may leave white space at page bottoms. Speeches taller than a full page still break.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
```

Check `app/Sources/ScreepubKit/DevicePreset.swift`: if presets are built by mutating `FormatSettings.defaults`, nothing to do; if any preset spells out a full `FormatSettings(...)` initializer call, add `keepSpeechesWhole: false,` in position there too (the compiler will point at every such site).

- [ ] **Step 4: Verify green.**

Run: `cd app && swift run -c release kit-check`
Expected: `kit-check: all passed`, including `canonical format-defaults.json decodes into FormatSettings` and `FormatSettings.defaults matches the canonical file` (the JSON gained the key in Task 1; the Swift default matches it).

- [ ] **Step 5: Commit.**

```bash
git add app/Sources/ScreepubKit/FormatSettings.swift app/Sources/ScreepubKit/ScriptSettings.swift app/Sources/ScreepubApp/AppSettings.swift app/Sources/ScreepubApp/ScreepubApp.swift app/Sources/KitCheck/main.swift
git commit -m "keepSpeechesWhole reaches the app: settings key, sidecar, Pages toggle"
```

---

### Task 7: Registry entries and the app bundle

**Files:**
- Modify: `docs/formatting-options-log.md`
- Rebuild: `app/dist/Screepub.app`

- [ ] **Step 1: Update registry entry #5a.** Find entry `#5a` (scene heading keep) and rewrite its mechanics paragraph to state: the keep is now `page-break-after: avoid` on `h2.scene-heading`, gated by `keepSceneHeadingWithScene` (default on); the old heading-plus-first-block wrapper was removed 2026-07-30 because it made the whole first block unbreakable and pushed half-page chunks on KFX and Apple Books (the half-empty-page report). Keep the entry number; append the date-stamped change note in the entry's existing style.

- [ ] **Step 2: Append three new entries** following the registry's numbering and voice. NOTE: #14 and #15 are TAKEN (scanned-PDF bail-out, not-a-screenplay guard). Use the suffix convention where an entry extends an existing story, flat numbers otherwise:

- `#8c keepSpeechesWhole` (extends the #8b cue-keep story) — opt-in atomic speeches: `.dialogue-block` gets `break-inside: avoid`; default off because the proven cue + first-line keep suffices and atomic blocks trade white space for never splitting a speech; speeches taller than a page still break bare (CSS avoid yields); the cue keep remains as a degradation layer either way.
- `#16 transitions never begin a page` — `p.transition { break-before: avoid }`, unconditional; the universal print rule (a transition belongs to the shot before it); see `docs/pagination-reference.md` §2 (note: that doc lives on branch worktree-device-map).
- `#10b tall dual exchanges fall back to sequential` (extends #10a dual dialogue) — a dual table is unbreakable on Kindle regardless of CSS, so an exchange whose taller column exceeds ~12 estimated rendered lines (at ~30 chars/line, constants in `src/epub/html.ts`) renders as two ordinary sequential speeches; short exchanges keep the side-by-side table.

Also note under #10b or #8c (one line): the MOBI dialect has no stylesheet, so none of these keeps apply there (structural limitation, unchanged).

- [ ] **Step 3: Rebuild the app bundle** (CLAUDE.md: it embeds the engine sidecar).

Run: `./app/build-app.sh`
Expected: `built: .../app/dist/Screepub.app`.

- [ ] **Step 4: Commit.**

```bash
git add docs/formatting-options-log.md
git commit -m "Registry: heading keep rewrite (#5a), atomic speeches, transition rule, dual fallback"
```

---

### Task 8: End-to-end verification

- [ ] **Step 1: Full suites.**

Run: `bun test && bunx tsc --noEmit && (cd app && swift run -c release kit-check)`
Expected: everything passes.

- [ ] **Step 2: epubcheck on generated output.**

```bash
bun src/cli.ts tests/fixtures/screenplay.pdf --out /tmp/pagespan-check
epubcheck /tmp/pagespan-check/*.epub
```

Expected: `No errors or warnings detected` (check `--help` first if `--out` differs; use the CLI's actual output-dir flag).

- [ ] **Step 3: Real-PDF sweep (only if root `fixtures/` exists on this machine).** Convert one real script with defaults and one with `--options` `{"keepSpeechesWhole": true}`; diff nothing — just confirm both convert cleanly and epubcheck passes.

- [ ] **Step 4: Device-truth check (manual, flag for the owner).** With the KFX toolchain ready, convert a script whose first scene opens with a long speech; open in Kindle Previewer and verify: (a) headings do not strand at page bottoms (`break-after: avoid` binds — the spec's named risk; if it does NOT bind, the fallback is a wrapper holding heading + first element only, and the registry gets the verdict), (b) pages fill markedly better than before, (c) with the toggle on, speeches move whole. Quick Apple Books pass for the same three.

- [ ] **Step 5: Final commit if verification touched anything, then report.** Summarize: what changed, suite results, epubcheck result, and the two manual checks awaiting device eyes.

---

## Self-review (done at planning time)

- Spec coverage: change 1 → Tasks 1, 2, 6; change 2 → Task 4; change 3 → covered by Task 4 (wrapper removal is what freed action); change 4 → Task 3; change 5 → Task 5; verification plan → Task 8; registry → Task 7. No gaps.
- Placeholders: none; every step carries code or an exact command.
- Type consistency: `keepSpeechesWhole` spelled identically across options.ts, JSON, Swift, and tests; `speechBlock(cells)` defined in Task 5 and used only there; dual state type change is contained in Task 5's snippets.
