// Tune: this script's own formatting. Nineteen knobs, saved beside the
// script, and re-rendered into Read as they move.
//
// The window decides nothing about what a knob MEANS: the engine owns the
// merge, the clamp and the storage (src/options.ts, src/settings/sidecar.ts).
// What this file decides is which control shows what, which knob is
// irrelevant right now, what one moved knob becomes on the way to the
// engine, and what the surface is allowed to claim about a save.
//
// The file is in two halves, the way convert.js and read.js are. Everything
// above the line is a pure function with no DOM in it, exercised directly by
// tests/desktop-ui.test.ts. Below the line is drawing, which holds no rule of
// its own and rides on the live run.
import { runEngine, argv, holdEngine } from './app.js';
import { inTurn, holders, beforeEveryTurn } from './book-queue.js';
import { el, clear, text } from './dom.js';
import { render as renderReader, splitPreview, dressFrame } from './read.js';

// ---------------------------------------------------------------- decisions

/** Why a knob can be on this surface and still not change the page behind
 *  it. Measured, not assumed — every one of the first eighteen was flipped
 *  against a cached .fountain and the re-rendered document compared byte for
 *  byte. The nineteenth, includeImages, decides whether html.ts writes a
 *  figure at all, which the source test in tests/desktop-ui.test.ts checks:
 *
 *  'live'      the preview changes the moment this moves.
 *  'book'      the BOOK changes and the preview cannot show it. The preview
 *              is the script itself; a generated title page is not in it.
 *  'reconvert' decided while the PDF is read, written into the script
 *              Screepub keeps, and therefore not recoverable from it. These
 *              are the four knobs src/fountain/serialize.ts consumes.
 *
 *  The temptation with the last group is to hide them. That trades one
 *  silence for another: the setting is still saved, still applies to the next
 *  conversion, and a reader who cannot find it cannot set it. Saying so is
 *  the honest move. */
export const EFFECTS = ['live', 'book', 'reconvert'];

/** The nineteen, grouped by what a reader is trying to change rather than by
 *  where the engine consumes them — with the one exception above, where
 *  where-it-is-consumed IS what a reader needs to know. `help` explains WHY,
 *  carried over from the Mac app's rail (app/Sources/ScreepubApp/
 *  ReaderRail.swift) rather than reinvented, because that is where the
 *  explaining already happened. */
export const GROUPS = [
  {
    id: 'page',
    title: 'The page',
    knobs: [
      {
        key: 'elementSpacingEm', label: 'Space between elements',
        kind: 'range', min: 0.4, max: 2, step: 0.1, unit: ' em',
      },
      {
        key: 'scenePageBreaks', label: 'Start each scene on a new page', kind: 'toggle',
        help: 'Makes every scene easy to find, and makes the book considerably longer.',
      },
      {
        key: 'keepSceneHeadingWithScene',
        label: 'Never end a page on a scene heading', kind: 'toggle',
        help: 'A heading alone at the foot of a page announces a scene and then makes you '
          + 'turn over to find it.',
      },
      {
        key: 'keepSpeechesWhole', label: 'Keep each speech on one page', kind: 'toggle',
        help: 'Stops a page turn landing in the middle of what someone is saying. The cost '
          + 'is a gap at the bottom of some pages. A speech longer than a whole page still '
          + 'has to break somewhere.',
      },
      {
        key: 'printSplitMinimums', label: 'Avoid stranded lines', kind: 'toggle',
        help: 'Stops a single line of a speech or a paragraph being left behind at the top '
          + 'or bottom of a page. Turning it off fits a little more onto each page. Not '
          + 'every e-reader obeys this one.',
      },
    ],
  },
  {
    id: 'dialogue',
    title: 'Dialogue',
    knobs: [
      {
        key: 'dialogueSideMarginPct', label: 'Dialogue margins',
        kind: 'range', min: 0, max: 30, step: 1, unit: '%',
      },
      {
        // A plural noun phrase, not a question. idleReason() composes this
        // label into "Only when <label> are indented.", so a label that reads
        // well alone but not in a sentence breaks the explanation beside two
        // OTHER knobs. "Where character names sit" did exactly that.
        key: 'cueAlignment', label: 'Character names', kind: 'choice',
        choices: [['centered', 'Centered'], ['indented', 'Indented']],
        help: 'Centred looks right at any screen size. Indented copies where they sit on a '
          + 'printed page, which only lines up at one width.',
      },
      {
        key: 'cueIndentPct', label: 'Character name indent',
        kind: 'range', min: 0, max: 60, step: 1, unit: '%',
        needs: { cueAlignment: 'indented' },
      },
      {
        key: 'parentheticalIndentPct', label: 'Parenthetical indent',
        kind: 'range', min: 0, max: 40, step: 1, unit: '%',
        needs: { cueAlignment: 'indented' },
      },
    ],
  },
  {
    id: 'text',
    title: 'The text',
    knobs: [
      {
        key: 'fontFamily', label: 'Typeface', kind: 'choice',
        choices: [['courier', 'Courier'], ['serif', 'Serif'], ['sans', 'Sans']],
      },
      {
        key: 'justifyText', label: 'Straighten the right edge', kind: 'toggle',
        help: 'Screenplays normally leave the right edge uneven. Straightening it opens up '
          + 'wide gaps between words in a column this narrow.',
      },
      {
        key: 'preserveFontShifts', label: "Keep the script's own type changes", kind: 'toggle',
        help: 'Some scripts set titles, inserts and on-screen text in a different typeface '
          + 'or size. This keeps them the way the script drew them; turning it off puts '
          + 'every line in one typeface.',
      },
    ],
  },
  {
    id: 'front',
    title: 'What the book carries',
    knobs: [
      {
        key: 'includeTitlePage', label: 'Title page', kind: 'toggle', effect: 'book',
        help: 'It appears in the finished book, not in the preview here, because the '
          + 'preview is the script itself.',
      },
      {
        key: 'showSceneNumbers', label: 'Scene numbers', kind: 'toggle',
        help: 'Only shows numbers the script already carried. Screepub never invents them: '
          + 'a numbered draft is a decision someone made, not a formatting choice.',
      },
      {
        // Live, unlike the title page beside it: a figure is markup html.ts
        // writes into the body, so the preview shows it. The engine keeps the
        // pictures and their places either way, so switching back needs no
        // new conversion.
        key: 'includeImages', label: 'Pictures from the script', kind: 'toggle',
        help: 'Shows the drawings and photos the PDF carried, in the book and in the '
          + 'preview, where they sat in the script. A script with no pictures makes the '
          + 'same book either way.',
      },
    ],
  },
  {
    // The four knobs src/fountain/serialize.ts consumes. They are decided
    // while the PDF is read and written into the script Screepub keeps, so
    // nothing downstream of that can put them back — which is why moving one
    // here cannot move the preview. Measured: re-rendering with each of the
    // four flipped produced a byte-identical document.
    id: 'pdf',
    title: 'Read from the PDF',
    effect: 'reconvert',
    note: 'These four are decided while the PDF is being read. Everything else re-renders '
      + 'from the script Screepub already read, so changing one here saves it for the next '
      + 'time you convert this PDF. It will not change what you see now.',
    knobs: [
      {
        key: 'rejoinSplitDialogue', label: 'Rejoin speeches split across pages', kind: 'toggle',
        help: 'A printed script breaks a long speech across two pages and marks it (MORE) '
          + "and (CONT'D). This stitches the halves back into one speech. Turning it off "
          + 'keeps the break exactly where the PDF had it.',
      },
      {
        key: 'contdMode', label: "(CONT'D) after a cue", kind: 'choice',
        choices: [['auto', 'Automatic'], ['strip', 'Remove all'], ['keep', 'Keep as written']],
      },
      {
        key: 'showPageMarkers', label: "The PDF's page numbers", kind: 'toggle',
        help: 'Marks where each page of the printed script ended, so a note about page 42 '
          + 'can still be found.',
      },
      {
        key: 'dualDialogue', label: 'People talking at once', kind: 'choice',
        choices: [['sideBySide', 'Side by side'], ['sequential', 'One after the other']],
        help: 'Side by side needs two columns, which is a sliver on a phone. One after the '
          + 'other reads anywhere.',
      },
    ],
  },
];

/** Every knob, flattened, in the order it is drawn. */
export const KNOBS = GROUPS.flatMap((group) =>
  group.knobs.map((knob) => ({ ...knob, effect: knob.effect ?? group.effect ?? 'live' })));

export const OPTION_KEYS = KNOBS.map((knob) => knob.key);

export function knobFor(key) {
  return KNOBS.find((knob) => knob.key === key) ?? null;
}

/** Why this knob does nothing right now, or null when it does something.
 *  Not a reason to hide it: a control that vanishes takes its own
 *  explanation with it, and the reader is left wondering where the cue
 *  indent went. It is shown, disabled, and told why. */
export function idleReason(knob, settings) {
  const needs = knob?.needs;
  if (!needs || settings === null || settings === undefined) return null;
  for (const [key, want] of Object.entries(needs)) {
    if (settings[key] === want) continue;
    const other = knobFor(key);
    const label = other?.choices?.find(([value]) => value === want)?.[1] ?? String(want);
    return `Only when ${(other?.label ?? key).toLowerCase()} are ${label.toLowerCase()}.`;
  }
  return null;
}

/** Everything that belongs beside a control right now: why it is doing
 *  nothing, then what it is for — in that order, because the reason it is
 *  greyed out is the more urgent of the two.
 *
 *  Drawing a knob and re-stating one both go through this, because they were
 *  once two answers and the second was never asked. The sentence was written
 *  at draw time only, so switching cues to Indented left "Only when character
 *  cues are indented." standing beside a control that had just become live —
 *  and a surface opened WITH cues indented had no sentence to show at all
 *  when they were switched back, leaving a greyed-out slider with no
 *  explanation and nothing for aria-describedby to point at. */
export function notesFor(knob, settings) {
  return [idleReason(knob, settings), knob?.help]
    .filter((note) => typeof note === 'string' && note.trim() !== '');
}

/** Whether a knob can ever have something to say. Its sentence element is
 *  drawn whenever this is true — empty and hidden if there is nothing to say
 *  yet — so there is always something for notesFor() to rewrite. */
export function canExplain(knob) {
  return Boolean(knob?.needs) || typeof knob?.help === 'string';
}

/** A read-out with no value to show: said in words, not stood in for by a
 *  dash. */
export const NOT_SET = 'not set';

/** What a control's read-out says. Tabular and unit-carrying: "20%" is an
 *  answer, "20" is a number. A value the engine never sends still has to
 *  render as something rather than as NaN. */
export function displayValue(knob, value) {
  // Number(null) is 0 and Number('') is 0: a missing value must read as
  // missing, not as a knob sitting at zero.
  if (value === null || value === undefined || value === '') return NOT_SET;
  const number = Number(value);
  if (!Number.isFinite(number)) return NOT_SET;
  const decimals = knob.step < 1 ? 1 : 0;
  return `${number.toFixed(decimals)}${knob.unit ?? ''}`;
}

/** One moved control, as a value the engine will accept — or null when it is
 *  not one, which is a change that must never be sent. A range arrives from
 *  the DOM as a STRING, and a string where the engine wants a number is
 *  discarded by the engine's own resolver in silence: the knob would appear
 *  to move and nothing would happen. The clamp is the same one src/options.ts
 *  applies, so the surface cannot offer a value the engine will throw away. */
export function coerceValue(knob, raw) {
  if (knob === null || knob === undefined) return null;
  if (knob.kind === 'toggle') return raw === true || raw === false ? raw : null;
  if (knob.kind === 'range') {
    const number = Number(raw);
    if (!Number.isFinite(number)) return null;
    return Math.min(knob.max, Math.max(knob.min, number));
  }
  const value = String(raw);
  return knob.choices.some(([choice]) => choice === value) ? value : null;
}

/** What is owed to the engine and has not been sent yet.
 *
 *  Every change accumulates here rather than replacing what is waiting. A
 *  reader who moves the margin and then the typeface inside one debounce
 *  window has changed two things; sending only the second SAVES only the
 *  second, and the engine — which overlays a partial on what is stored —
 *  would hand back settings with the first one missing and the surface would
 *  redraw it away. */
export function mergePending(pending, key, value) {
  return { ...pending, [key]: value };
}

export function isPending(pending) {
  return Object.keys(pending ?? {}).length > 0;
}

/** What is owed after a save that did not happen. `pending` is cleared
 *  before the engine is asked — that is what makes a knob moved DURING a
 *  save land in the next one — so a failure has to put back what the
 *  attempt carried, or the screen would show values the engine never stored
 *  and nothing would be left to store them. Anything moved since the attempt
 *  started wins: it is the newer answer about that knob. */
export function restorePending(owed, pending) {
  return { ...owed, ...pending };
}

/** The engine's settings answer, or null if it is not one. Checked key by
 *  key against the nineteen and against each knob's own kind: a surface that
 *  trusted a malformed answer would draw an empty select or a slider at NaN
 *  and give a reader no idea why. Extra keys are carried through untouched —
 *  a future engine knob this window has never heard of must survive a round
 *  trip rather than being quietly dropped from the script's settings. */
export function settingsFrom(answer) {
  const settings = answer?.settings;
  if (typeof settings !== 'object' || settings === null || Array.isArray(settings)) return null;
  for (const knob of KNOBS) {
    const value = settings[knob.key];
    if (knob.kind === 'toggle' && typeof value !== 'boolean') return null;
    if (knob.kind === 'range' && !Number.isFinite(value)) return null;
    if (knob.kind === 'choice' && !knob.choices.some(([choice]) => choice === value)) return null;
  }
  return { ...settings };
}

/** The named starting points, from the engine's own list. A preset whose
 *  settings do not survive settingsFrom() is left out rather than offered as
 *  a button that would half-apply. */
export function presetsFrom(answer) {
  const presets = answer?.presets;
  if (!Array.isArray(presets)) return [];
  return presets
    .map((preset) => ({
      // Carried so the surface can mark the one the script is actually on.
      // Without it the two presets are just two buttons, and the one that
      // equals the defaults looks like a button that does nothing.
      id: typeof preset?.id === 'string' ? preset.id : '',
      displayName: typeof preset?.displayName === 'string' ? preset.displayName : '',
      settings: settingsFrom(preset),
    }))
    .filter((preset) => preset.displayName !== '' && preset.settings !== null);
}

/** Which preset this script's settings currently equal, or null when they
 *  have been tuned away from every one of them.
 *
 *  The engine answers this by comparing values, not by remembering a name it
 *  was handed — `src/settings/presets.ts` says why: applying a preset stores
 *  no identity, so equality is the only honest answer, and a remembered name
 *  would go on claiming "Kindle e-ink" after the first knob moved.
 *
 *  The window ignored this field entirely until 2026-09-21. That is what made
 *  the row read as arbitrary: one of the two presets is identical to the
 *  defaults, so on a fresh script it appears to do nothing, and without
 *  "you are here" there was no way to tell a no-op from a confirmation. */
export function currentPreset(answer) {
  const id = answer?.preset;
  return typeof id === 'string' && id !== '' ? id : null;
}

export function sameSettings(a, b) {
  if (a === null || b === null || a === undefined || b === undefined) return a === b;
  return OPTION_KEYS.every((key) => a[key] === b[key]);
}

/** What the surface is allowed to claim, and when. "Saved" is a claim about
 *  a file on disk, so it is never shown while a change is still owed: a
 *  reader who walks away on the word "saved" has to be right. */
export const STATUS = {
  idle: '',
  pending: 'Changed. Saving in a moment…',
  saving: 'Saving, and rebuilding the book to match…',
  saved: 'Saved. The book on disk matches what you see.',
};

/** What a save waiting its turn on the book says, by what holds the book
 *  (book-queue.js's labels). "In a moment" would not be true while a
 *  half-minute KFX build has the book. A save REWRITES the book, so it waits
 *  for every turn running on it, and readers run side by side: the order
 *  here is which one it names when several are, the longest wait first. The
 *  Send page starts a Kindle file build as it opens, so that is the usual
 *  one. */
export const WAITING = {
  kindle: 'Waiting for the Kindle file to finish building…',
  'kindle-mobi': 'Waiting for the Kindle file to finish building…',
  convert: 'Waiting for the conversion to finish…',
  send: 'Waiting for the send to finish…',
  copy: 'Waiting for the copy to finish…',
};

export const NO_MESSAGE = 'The engine refused the change without saying why.';

/** The status line for a phase. `message` is the engine's sentence for
 *  'failed', and what holds the book for 'waiting': one label, or every one
 *  running (holders()), named by WAITING's order. This page's own earlier
 *  save, and a label with no words, are still "in a moment". */
export function statusFor(phase, message) {
  if (phase === 'failed') {
    const said = typeof message === 'string' ? message.trim() : '';
    return { line: said === '' ? NO_MESSAGE : said, bad: true };
  }
  if (phase === 'waiting') {
    const labels = [].concat(message);
    const named = Object.keys(WAITING).find((label) => labels.includes(label));
    return { line: named === undefined ? STATUS.pending : WAITING[named], bad: false };
  }
  return { line: STATUS[phase] ?? '', bad: false };
}

/** There is no notice for a surface with no script open: main.js disables
 *  this tab whenever none is, exactly as it does for Read, so a reader can
 *  never be standing here with nothing converted. The one state that IS
 *  reachable and is not the knobs is a script whose settings could not be
 *  read — a sidecar on a disk that went away. */
export const FAULT = {
  slug: 'Settings out of reach',
  line: 'Screepub could not read this script’s settings. The book itself is fine and can '
    + 'still be sent; converting the PDF again is the way to get the knobs back.',
  way: 'Convert it again',
};

export const LEDE = 'These settings belong to this script alone. They are kept with it in '
  + 'your library, never beside the PDF you dropped, and the book on disk is rebuilt to '
  + 'match, so what you send is what you see.';

/** "Start from" said nothing about what pressing one does. It does not nudge
 *  a setting or set a baseline to build on: it REPLACES all nineteen. The
 *  SwiftUI version said so on screen and this one did not. */
export const PRESET_LABEL = 'Load a preset';
export const PRESET_NOTE = 'Overwrites every setting below.';

/** What the pane shows before there is anything to tune: FAULT only when
 *  THIS draw is itself reporting a load that failed. load() already tells
 *  it that, in its catch and its own `settings === null` branch, by passing
 *  a bad status down to draw(). Otherwise a quiet reading caption, which
 *  covers both a pane that has not asked yet (mount, scriptChanged) and one
 *  still waiting on an answer, because nothing else redraws in between.
 *  Treating a bare `settings === null` as failure, unconditionally, was the
 *  defect: every ordinary look at Settings passes through null before
 *  load() answers, so a reader saw the fault screen flash on the way in
 *  every single time. */
export function emptyPaneMode(settings, status) {
  if (settings !== null) return 'ready';
  return status?.bad === true ? 'fault' : 'reading';
}

export const READING = 'Reading this script’s settings…';

/** The engine's `settings` answer also carries the app-wide defaults a NEW
 *  script would start from: `appDefaults` (what a new script gets today) and
 *  `defaults` (Screepub's own, unconditionally, the same field settingsFrom
 *  already reads as "this script's shipped defaults"). Both are validated
 *  the identical way a script's own settings are, through settingsFrom
 *  itself: an app default that fails the nineteen-knob check is one this
 *  window cannot safely claim anything about. */
export function appDefaultsFrom(answer) {
  return settingsFrom({ settings: answer?.appDefaults });
}

export function shippedDefaultsFrom(answer) {
  return settingsFrom({ settings: answer?.defaults });
}

/** What "Use these for new scripts" or "Reset new scripts to Screepub's
 *  defaults" should show once app-settings --set has answered: the fresh
 *  app defaults and Screepub's own, both validated through settingsFrom, or
 *  a message for the confirmation line. Mirrors convert.js's libraryAfter:
 *  same shape, same reason, a different field pair. */
export function appSettingsAfter(answer) {
  const appDefaults = settingsFrom({ settings: answer?.formatDefaults });
  const shippedDefaults = settingsFrom({ settings: answer?.shippedDefaults });
  if (appDefaults !== null && shippedDefaults !== null) {
    return { ok: true, appDefaults, shippedDefaults };
  }
  const message = typeof answer?.error?.message === 'string' ? answer.error.message.trim() : '';
  return { ok: false, message: message === '' ? NO_MESSAGE : message };
}

/** One sentence saying which defaults a new script starts from, compared
 *  knob by knob through sameSettings, the same equality the engine itself
 *  uses and the same reason currentPreset() above compares rather than
 *  remembers a name: nothing stores WHICH defaults a reader chose, only
 *  what they equal now. */
export function defaultsCaption(appDefaults, shippedDefaults) {
  return sameSettings(appDefaults, shippedDefaults)
    ? 'New scripts start from Screepub’s own defaults.'
    : 'New scripts start from your own defaults.';
}

/** Whether there is anything for "Reset new scripts to Screepub's defaults"
 *  to do. Not offered when the app defaults could not be read at all: a
 *  button that resets an unknown quantity is a button that might announce a
 *  change that never happens. */
export function canResetDefaults(appDefaults, shippedDefaults) {
  return appDefaults !== null && shippedDefaults !== null
    && !sameSettings(appDefaults, shippedDefaults);
}

/** The --set value behind both buttons: this script's current settings
 *  become the app defaults, or `null` sends the app back to Screepub's own.
 *  One value differs, one call shape, the same trade convert.js's
 *  libraryChangeArgs makes for Change and Reset on the library folder. */
export function defaultsWriteArgs(next) {
  return JSON.stringify({ formatDefaults: next });
}

export const USE_DEFAULTS_LABEL = 'Use these for new scripts';
export const RESET_DEFAULTS_LABEL = 'Reset new scripts to Screepub’s defaults';
export const USE_DEFAULTS_NOTE = 'New scripts will start from these settings.';
export const RESET_DEFAULTS_NOTE = 'New scripts will start from Screepub’s own defaults.';

/** What a defaults write becomes once app-settings --set has answered, or
 *  the attempt has thrown: applied, refused, or dropped because the page has
 *  since moved to a different script. `mine` is the era captured when the
 *  write began; `current` is what era() reads at the moment the answer
 *  arrives. Pulled out of the drawing code so the era guard, the success
 *  path and the refusal path are each one branch a test can drive without a
 *  DOM: a write that outlives its script must not paint a caption onto
 *  whatever script replaced it, the same rule flush() already follows for
 *  this script's own settings. */
export function defaultsWriteOutcome(mine, current, answer, confirmedMessage) {
  if (mine !== current) return { applied: false, stale: true, message: '' };
  const result = appSettingsAfter(answer);
  if (result.ok) {
    return {
      applied: true, stale: false, message: confirmedMessage,
      appDefaults: result.appDefaults, shippedDefaults: result.shippedDefaults,
    };
  }
  return { applied: false, stale: false, message: result.message };
}

/** The app-wide choice beside the defaults foot: whether a PDF converted
 *  from now on keeps the settings it started from (the engine's "pin"), or
 *  follows the defaults until it is tuned. Spec:
 *  docs/superpowers/specs/2026-09-24-keep-script-settings-choice-design.md.
 *  The words are the owner's. `id` is the radio's own element id, which its
 *  label and its line are tied to. */
export const KEEP_CHOICE = {
  legend: 'When a PDF is converted',
  options: [
    {
      keep: true, id: 'keep-script-settings-keep', label: 'Keep its settings',
      line: 'The script keeps the settings it was made with. Changing the defaults later '
        + 'won’t change it.',
    },
    {
      keep: false, id: 'keep-script-settings-follow', label: 'Follow the defaults',
      line: 'Scripts you haven’t tuned change when the defaults do. Scripts that already have '
        + 'their own settings keep them.',
    },
  ],
};

/** Said once a choice is stored. It names the one thing the radios cannot:
 *  nothing already converted changes. */
export const KEEP_SAVED_NOTE = 'Saved. It applies to PDFs you convert from now on.';

/** The choice off a settings answer (or an app-settings one: both carry
 *  `keepScriptSettings`), or null when it is not a boolean. Null draws no
 *  choice at all, the same call drawDefaultsFoot() makes for a malformed
 *  answer: a radio checked by guesswork would claim something about the
 *  reader's settings file that nobody read. */
export function keepChoiceFrom(answer) {
  const value = answer?.keepScriptSettings;
  return typeof value === 'boolean' ? value : null;
}

/** The --set value behind either radio. */
export function keepWriteArgs(keep) {
  return JSON.stringify({ keepScriptSettings: keep });
}

/** What one choice write becomes once app-settings --set has answered.
 *  `mine`/`current` are the era when the write began and now, the same
 *  guard defaultsWriteOutcome() uses: another script's page gets nothing.
 *  `seq`/`latest` are this write's place in the queue and the newest
 *  choice's: a write overtaken by a newer choice still reports what the
 *  engine STORED (so a later refusal can put the radios back to the truth),
 *  but the radios and the note belong to the newer write. */
export function keepWriteOutcome(mine, current, seq, latest, answer) {
  if (mine !== current) return { stale: true, stored: null };
  const stored = answer?.ok === true ? keepChoiceFrom(answer) : null;
  if (seq !== latest) return { stale: true, stored };
  if (stored !== null) return { stale: false, applied: true, stored, message: KEEP_SAVED_NOTE };
  const said = typeof answer?.error?.message === 'string' ? answer.error.message.trim() : '';
  return { stale: false, applied: false, stored: null, message: said === '' ? NO_MESSAGE : said };
}

// ------------------------------------------------------------------ drawing

let ctx = null;
let pane = null;
let settings = null;
let presets = [];
let onPreset = null;
let presetsBox = null;
// The app-wide defaults a NEW script starts from, and Screepub's own, loaded
// alongside this script's own settings (same answer, appDefaultsFrom and
// shippedDefaultsFrom read two more fields off it) because that is the one
// engine round trip Settings already makes; a second probe just to draw a
// caption would be the well's old library-line mistake repeated.
//
// applyDefaultsFootState() is the one function that writes these four onto
// the foot's own nodes (also declared here, not local to drawDefaultsFoot):
// a redraw that happens for some other reason while a write is in flight (a
// preset click's applyAll, or flush()'s own correcting redraw) still shows
// the true busy/message state, because drawDefaultsFoot() calls the same
// function right after building fresh nodes; and a write's own update lands
// on the SAME nodes a reader may still have a hand on, rather than a
// replacement that would drop their focus and never announce itself.
let appDefaults = null;
let shippedDefaults = null;
let defaultsBusy = false;
let defaultsNote = '';
let defaultsCaptionEl = null;
let defaultsNoteEl = null;
let defaultsUseButton = null;
let defaultsResetButton = null;
// "When a PDF is converted", drawn beside the foot and kept the same way:
// built once per full draw(), then updated IN PLACE by
// applyKeepChoiceState(), so the radio a reader is standing on is never
// swapped out from under them. `keepSettings` is what the engine last said
// is stored; `keepShown` is what the radios show, which runs ahead of it
// while the reader's newest choice is still on its way to the engine.
// `keepSeq` and `keepRunning` are NOT per script and are never reset: the
// setting is the app's, and a choice made on one script's page is still
// the reader's choice once another script is open.
let keepSettings = null;
let keepShown = null;
let keepNote = '';
let keepRadios = null;
let keepNoteEl = null;
let keepSeq = 0;
let keepRunning = Promise.resolve();
let previewFrame = null;
let previewCss = '';
let loaded = false;
let timer = null;
/** The release for the hold schedule() takes while a moved knob waits out
 *  SETTLE_MS, or null when none is held. Between a knob moving and its
 *  save's engine call starting, no engine call is running at all, so an
 *  update restart already waiting on app.js's whenIdle() would otherwise be
 *  free to fire and drop the change (the known gap in
 *  docs/superpowers/specs/2026-09-23-update-notice-and-window-drag-design.md,
 *  Part 3, closed 2026-09-24). One hold covers however many knobs move
 *  inside one settle; releaseHold() is the only thing that lets it go. */
let hold = null;
let pending = {};
let statusLine = null;
let controls = new Map();
/** Which script these knobs belong to; scriptChanged() moves it on. A save
 *  waiting its turn on its book checks it before it starts (settle()), and
 *  one already running, whose engine call cannot be taken back, checks it
 *  before it paints anything (flush()): converting a second PDF while a
 *  save is waiting or in flight must neither run the second script's
 *  changes on the first one's turn nor put the first script's pages back
 *  into the reader. */
let era = 0;

/** Long enough that a dragged slider is one conversion rather than a
 *  hundred, short enough that a single click does not feel ignored. */
const SETTLE_MS = 300;

export function mount(node, context) {
  pane = node;
  ctx = context;
  draw();
}

/** The script on screen changed, or went away: these knobs belong to the
 *  other one. */
export function scriptChanged() {
  era += 1;
  loaded = false;
  settings = null;
  presets = [];
  appDefaults = null;
  shippedDefaults = null;
  defaultsBusy = false;
  defaultsNote = '';
  defaultsCaptionEl = null;
  defaultsNoteEl = null;
  defaultsUseButton = null;
  defaultsResetButton = null;
  keepSettings = null;
  keepShown = null;
  keepNote = '';
  keepRadios = null;
  keepNoteEl = null;
  pending = {};
  clearTimeout(timer);
  timer = null;
  // What was owed belonged to the other script and has just been dropped,
  // so nothing is owed any more: the restart need not wait for it.
  releaseHold();
  draw();
}

export async function show() {
  if (loaded || ctx.state.script === null) return;
  loaded = true;
  await load();
}

async function load() {
  const script = ctx.state.script;
  if (!script?.fountainPath) {
    draw(statusFor('failed', FAULT.line));
    return;
  }
  try {
    const answer = await runEngine(argv.settings(script.fountainPath));
    settings = settingsFrom(answer);
    presets = presetsFrom(answer);
    onPreset = currentPreset(answer);
    appDefaults = appDefaultsFrom(answer);
    shippedDefaults = shippedDefaultsFrom(answer);
    keepSettings = keepChoiceFrom(answer);
    keepShown = keepSettings;
  } catch (err) {
    settings = null;
    // Openable again: a sidecar that could not be read once — a disk that
    // was not there yet, an engine that did not start — must not strand
    // this surface on the fault screen for the life of the script.
    loaded = false;
    draw(statusFor('failed', err.message));
    return;
  }
  if (settings === null) {
    loaded = false;
    draw(statusFor('failed', NO_MESSAGE));
    return;
  }
  script.settings = settings;
  draw();
}

function draw(status) {
  clear(pane);
  controls = new Map();
  statusLine = null;
  if (ctx.state.script === null) return;

  const mode = emptyPaneMode(settings, status);
  if (mode === 'reading') {
    // Not FAULT: nothing has failed, load() just has not answered yet, or
    // has not been asked yet. mount() and scriptChanged() both draw before
    // show() ever calls load(). A reader opening Settings on an ordinary
    // script used to see the fault screen for exactly this long, every time.
    pane.append(el('p', { class: 'caption' }, READING));
    return;
  }
  if (mode === 'fault') {
    // Through el(), which drops a null child, rather than straight onto the
    // pane, which renders one as the word "null". `status` is always a bad
    // one here: that is what emptyPaneMode used to decide this branch.
    pane.append(el('div', { class: 'fault-body-block' },
      el('h2', { class: 'slug' }, FAULT.slug),
      el('p', { class: 'prose' }, FAULT.line),
      el('p', { class: 'caption bad' }, status.line),
      el('div', { class: 'read-ways' },
        el('button', {
          type: 'button', class: 'btn btn-outline', onclick: () => ctx.convertAgain(),
        }, FAULT.way)),
    ));
    return;
  }

  statusLine = el('p', { class: 'caption tune-status', role: 'status' }, '');
  pane.append(
    el('h2', { class: 'slug' }, 'This script’s settings'),
    el('p', { class: 'prose' }, LEDE),
    el('div', { class: 'tune-split' },
      el('div', { class: 'tune-knobs' },
        (presetsBox = drawPresets()),
        ...GROUPS.map(drawGroup),
        statusLine,
        el('div', { class: 'tune-foot' }, drawDefaultsFoot(), drawKeepChoice()),
      ),
      drawPreview(),
    ),
  );
  say(status ?? statusFor(isPending(pending) ? 'pending' : 'idle'));
  renderPreview(ctx.state.script?.previewHtml);
}

/** The script, beside the knobs that change it.
 *
 *  Only the `live` knobs can move this — the effect classification above says
 *  which, and it was measured rather than guessed. The `book` and `reconvert`
 *  ones cannot, and they say so themselves rather than being hidden: a knob
 *  that silently does nothing here is worse than one that explains why.
 *
 *  A plain frame, not the reader. It carries no scene rail, keeps no reading
 *  place and takes no keyboard: this is a swatch, not somewhere you read. The
 *  one thing it shares with the reader is dressFrame(), because the CSP dance
 *  that gets the engine's stylesheet into the document is the piece that must
 *  never exist twice. */
function drawPreview() {
  previewFrame = el('iframe', {
    class: 'tune-preview-frame',
    title: 'Preview',
    // Same grant the reader's frame gets, and no more: same-origin so the
    // parent can adopt a stylesheet into it, nothing that lets the document
    // run anything.
    sandbox: 'allow-same-origin',
    // Not a Tab stop. There is nothing to do inside it.
    tabindex: '-1',
    'aria-hidden': 'true',
  });
  previewFrame.addEventListener('load', () => dressFrame(previewFrame, previewCss));
  return el('div', { class: 'tune-preview' },
    el('p', { class: 'state-label' }, 'Preview'),
    previewFrame);
}

/** Show a document in the preview. Safe to call before the frame exists and
 *  safe to call with nothing, which is what happens on the fault screen. */
function renderPreview(previewHtml) {
  if (previewFrame === null || previewHtml === undefined || previewHtml === null) return;
  const parts = splitPreview(previewHtml, new DOMParser());
  previewCss = parts.css;
  previewFrame.setAttribute('srcdoc', parts.html);
}

function drawPresets() {
  if (presets.length === 0) return null;
  return el('div', { class: 'presets' },
    el('p', { class: 'state-label presets-label' }, PRESET_LABEL),
    el('div', { class: 'preset-row' },
      ...presets.map((preset) => {
        // The one the script is already on is marked and inert. It is what
        // turns a button that appears to do nothing — the Kindle preset IS
        // the defaults — into a statement about where you are.
        const on = preset.id !== '' && preset.id === onPreset;
        return el('button', {
          type: 'button',
          class: `btn btn-outline btn-small${on ? ' preset-on' : ''}`,
          disabled: on,
          'aria-current': on ? 'true' : null,
          onclick: () => applyAll(preset.settings),
        }, preset.displayName);
      })),
    el('p', { class: 'caption presets-note' }, PRESET_NOTE));
}

/** The foot of the Settings page: which defaults a NEW script starts from,
 *  and a way to change that. Built once per full draw() and then left ALONE:
 *  a write updates the four nodes below (defaultsCaptionEl, defaultsNoteEl,
 *  defaultsUseButton, defaultsResetButton) in place, through
 *  applyDefaultsFootState(), rather than rebuilding them. Two reasons, both
 *  the same rule the rest of this file already follows for a knob's own
 *  state(): replacing the button a reader just pressed would drop their
 *  focus onto the body the moment they clicked it, and a role="status" node
 *  has to already be sitting in the page, empty, before its text changes
 *  for most screen readers to announce the change at all. A live region
 *  that appears already full announces nothing.
 *
 *  Neither button ever calls argv.settings(: this surface's OWN save path
 *  (change() through schedule() to flush()) is untouched by either one,
 *  because what they write is the APP's defaults, not this script's
 *  settings. */
function drawDefaultsFoot() {
  // A malformed answer, the engine's contract broken, not a reader's doing,
  // leaves nothing this block could honestly claim. Absent rather than
  // wrong, the same call drawPresets() makes with an empty list.
  if (appDefaults === null || shippedDefaults === null) {
    defaultsCaptionEl = null;
    defaultsNoteEl = null;
    defaultsUseButton = null;
    defaultsResetButton = null;
    return null;
  }
  defaultsCaptionEl = el('p', { class: 'caption' });
  defaultsNoteEl = el('p', { class: 'caption defaults-note', role: 'status' });
  defaultsUseButton = el('button', {
    type: 'button', class: 'btn-quiet',
    onclick: () => writeDefaults(settings, USE_DEFAULTS_NOTE),
  }, USE_DEFAULTS_LABEL);
  defaultsResetButton = el('button', {
    type: 'button', class: 'btn-quiet',
    onclick: () => writeDefaults(null, RESET_DEFAULTS_NOTE),
  }, RESET_DEFAULTS_LABEL);
  applyDefaultsFootState();
  return el('div', { class: 'tune-defaults' },
    defaultsCaptionEl,
    el('div', { class: 'tune-defaults-row' }, defaultsUseButton, defaultsResetButton),
    defaultsNoteEl);
}

/** The one place that writes appDefaults/shippedDefaults/defaultsBusy/
 *  defaultsNote onto the four nodes drawDefaultsFoot() built: called right
 *  after building them, so the first paint agrees with every later refresh,
 *  and by writeDefaults() below on every way out. Silent when the foot is
 *  not on screen (defaultsCaptionEl is null: the fault or reading state, or
 *  a malformed answer) rather than throwing partway through a write. */
function applyDefaultsFootState() {
  if (defaultsCaptionEl === null) return;
  text(defaultsCaptionEl, defaultsCaption(appDefaults, shippedDefaults));
  defaultsUseButton.disabled = defaultsBusy;
  defaultsResetButton.disabled = defaultsBusy;
  defaultsResetButton.hidden = !canResetDefaults(appDefaults, shippedDefaults);
  text(defaultsNoteEl, defaultsNote);
  defaultsNoteEl.hidden = defaultsNote === '';
}

/** `next` is this script's current settings (Use these) or null (Reset).
 *  Both buttons go quiet for the whole round trip, not just the one
 *  pressed: two writes racing each other is exactly what disabling only the
 *  clicked button would allow. `mine` follows the same era guard flush()
 *  uses, because this write, like that one, can outlive the script it was
 *  started from: the reader may have opened a different script's Settings,
 *  whose own foot is what is actually on screen, before this one's engine
 *  call returns, and that foot's nodes are what module state now refers to
 *  (a stale `era` is the only thing that still says whose write this was).
 *  defaultsWriteOutcome makes the era check, the success path and the
 *  refusal path each one branch a test can drive without a DOM; this
 *  function is the thin glue that applies its answer to the module state
 *  and updates the foot's own nodes to match. */
async function writeDefaults(next, confirmed) {
  const mine = era;
  defaultsBusy = true;
  defaultsNote = '';
  applyDefaultsFootState();
  try {
    const answer = await runEngine(argv.appSettings(defaultsWriteArgs(next)));
    const outcome = defaultsWriteOutcome(mine, era, answer, confirmed);
    if (outcome.stale) return;
    if (outcome.applied) {
      appDefaults = outcome.appDefaults;
      shippedDefaults = outcome.shippedDefaults;
    }
    defaultsNote = outcome.message;
  } catch (err) {
    if (era !== mine) return;
    defaultsNote = err.message;
  } finally {
    if (era === mine) {
      defaultsBusy = false;
      applyDefaultsFootState();
    }
  }
}

/** "When a PDF is converted": a real radio group, beside the defaults foot
 *  because it decides what those defaults mean for a script converted
 *  later. A fieldset and legend name the group; two native radios sharing
 *  one name give it one Tab stop and arrow keys between the options; each
 *  has a label and its line tied on with aria-describedby. Absent, like the
 *  foot, when the answer did not say what is stored. */
function drawKeepChoice() {
  if (keepSettings === null) {
    keepRadios = null;
    keepNoteEl = null;
    return null;
  }
  keepRadios = new Map();
  const options = KEEP_CHOICE.options.map((option) => {
    const lineId = `${option.id}-line`;
    const input = el('input', {
      type: 'radio', id: option.id, name: 'keep-script-settings', class: 'keep-radio',
      'aria-describedby': lineId,
      onchange: () => chooseKeep(option.keep),
    });
    keepRadios.set(option.keep, input);
    return el('div', { class: 'keep-option' },
      input,
      el('label', { for: option.id, class: 'keep-label' }, option.label),
      el('p', { class: 'caption keep-line', id: lineId }, option.line));
  });
  keepNoteEl = el('p', { class: 'caption keep-note', role: 'status' });
  applyKeepChoiceState();
  return el('fieldset', { class: 'keep-choice' },
    el('legend', { class: 'state-label keep-legend' }, KEEP_CHOICE.legend),
    ...options,
    keepNoteEl);
}

/** The one place that writes keepShown and keepNote onto the group's nodes,
 *  on the first paint and on every answer after it. Silent when the group
 *  is not on screen. */
function applyKeepChoiceState() {
  if (keepRadios === null) return;
  for (const [keep, input] of keepRadios) input.checked = keep === keepShown;
  text(keepNoteEl, keepNote);
  keepNoteEl.hidden = keepNote === '';
}

/** A radio was chosen. The write is queued behind any still out, rather
 *  than the radios going quiet the way the foot's buttons do: an arrow key
 *  both moves focus and changes the choice, and disabling the radio a
 *  keyboard reader is standing on would drop their place. Queued, two
 *  writes cannot finish in the wrong order; numbered, only the newest one
 *  paints. */
function chooseKeep(keep) {
  if (keep === keepShown) return;
  keepShown = keep;
  keepNote = '';
  applyKeepChoiceState();
  const mine = era;
  const seq = ++keepSeq;
  keepRunning = keepRunning.catch(() => {}).then(() => writeKeep(keep, mine, seq));
}

/** Send one choice, unless a newer one has been made while it waited its
 *  turn (the newer one says what the reader wants, and will be sent next).
 *  Still sent when the page has moved to another script since: the setting
 *  is the app's, and the reader chose it. Only the painting follows the
 *  era guard, through keepWriteOutcome(). */
async function writeKeep(keep, mine, seq) {
  if (seq !== keepSeq) return;
  let outcome;
  try {
    const answer = await runEngine(argv.appSettings(keepWriteArgs(keep)));
    outcome = keepWriteOutcome(mine, era, seq, keepSeq, answer);
  } catch (err) {
    outcome = era !== mine || seq !== keepSeq
      ? { stale: true, stored: null }
      : { stale: false, applied: false, stored: null, message: err.message };
  }
  if (outcome.stored !== null) keepSettings = outcome.stored;
  if (outcome.stale) return;
  keepShown = keepSettings;
  keepNote = outcome.message;
  applyKeepChoiceState();
}

/** Which groups arrive open. Only the first: five shut boxes is a surface
 *  with nothing on it, and every control open at once is the wall this
 *  replaced.
 *
 *  Exported so the rule is a fact rather than an inline literal, and so a
 *  later "remember what was open" can be added without hunting for where the
 *  decision was made. */
export function groupStartsOpen(index) {
  return index === 0;
}

function drawGroup(group, index) {
  // <details> rather than a toggle of our own: the open state, the keyboard
  // and the announcement all come from the platform. Folding, never
  // dropping — all five groups and all nineteen knobs are still here, because
  // a setting that cannot be found still applies to every conversion.
  return el('details', { class: 'knob-group', open: groupStartsOpen(index) ? '' : null },
    el('summary', { class: 'subslug knob-group-title' }, group.title),
    group.note ? el('p', { class: 'caption knob-group-note' }, group.note) : null,
    ...group.knobs.map((knob) => drawKnob(knobFor(knob.key))));
}

function drawKnob(knob) {
  const id = `knob-${knob.key}`;
  const value = settings[knob.key];
  // Drawn whenever this knob could ever explain itself, even if it has
  // nothing to say yet: state() below is what fills it in, now and every
  // time the setting it depends on moves.
  const why = canExplain(knob)
    ? el('p', { class: 'caption knob-why', id: `${id}-why` })
    : null;

  const row = el('div', { class: `knob knob-${knob.kind}` },
    el('label', { for: id, class: 'knob-label' }, knob.label));
  let input;

  if (knob.kind === 'range') {
    const readOut = el('span', { class: 'knob-value' }, displayValue(knob, value));
    input = el('input', {
      type: 'range', id, class: 'knob-slider',
      min: knob.min, max: knob.max, step: knob.step, value: String(value),
      oninput: (event) => {
        text(readOut, displayValue(knob, event.target.value));
        change(knob, event.target.value);
      },
    });
    row.append(readOut, input);
    controls.set(knob.key, { input, readOut, why, knob });
  } else if (knob.kind === 'toggle') {
    input = el('input', {
      type: 'checkbox', id, class: 'knob-box',
      onchange: (event) => change(knob, event.target.checked),
    });
    input.checked = value === true;
    row.append(input);
    controls.set(knob.key, { input, why, knob });
  } else {
    input = el('select', {
      id, class: 'knob-select',
      onchange: (event) => change(knob, event.target.value),
    }, ...knob.choices.map(([choice, label]) => el('option', { value: choice }, label)));
    input.value = String(value);
    row.append(input);
    controls.set(knob.key, { input, why, knob });
  }

  if (why !== null) row.append(why);
  state(controls.get(knob.key), row);
  return row;
}

/** Put one control into the state the current settings ask for: live or
 *  idle, and the sentences that go with that. The ONE place that decides it,
 *  so a knob drawn idle and a knob that becomes idle cannot disagree. */
function state({ input, why, knob }, row = input.closest('.knob')) {
  const notes = notesFor(knob, settings);
  const idle = idleReason(knob, settings) !== null;
  input.disabled = idle;
  row?.classList.toggle('knob-idle', idle);
  if (why === null) return;
  text(why, notes.join(' '));
  // An empty <p> is not something to point a screen reader at.
  why.hidden = notes.length === 0;
  if (notes.length === 0) input.removeAttribute('aria-describedby');
  else input.setAttribute('aria-describedby', why.id);
}

/** A preset overwrites every knob at once, so the whole surface is redrawn
 *  and the whole settings object is what gets owed to the engine. */
function applyAll(next) {
  const changed = {};
  for (const key of OPTION_KEYS) {
    if (settings[key] !== next[key]) changed[key] = next[key];
  }
  if (Object.keys(changed).length === 0) return;
  // The script is now on whichever preset these settings are, by equality —
  // the same rule the engine uses. Matched on the values rather than on the
  // button that was pressed, so applying a preset that happens to equal
  // another one cannot leave the wrong one marked.
  onPreset = presets.find((p) => OPTION_KEYS.every((k) => p.settings[k] === next[k]))?.id ?? null;
  settings = { ...settings, ...changed };
  ctx.state.script.settings = settings;
  pending = { ...pending, ...changed };
  draw();
  schedule();
}

function change(knob, raw) {
  const value = coerceValue(knob, raw);
  if (value === null || settings[knob.key] === value) return;
  // One moved knob takes the script off whatever preset it was on. The mark
  // has to go with it or the surface keeps claiming a name that stopped
  // being true, which is the exact failure src/settings/presets.ts refuses
  // to commit by storing equality instead of an identity.
  const wasOn = onPreset;
  onPreset = null;
  if (wasOn !== null) refreshPresets();
  settings = { ...settings, [knob.key]: value };
  ctx.state.script.settings = settings;
  pending = mergePending(pending, knob.key, value);
  // Alignment decides whether two other knobs mean anything, and those two
  // have to say so themselves.
  if (knob.key === 'cueAlignment') refreshIdle();
  say(statusFor('pending'));
  schedule();
}

/** Swap the preset row for a fresh one, so the "you are here" mark can move
 *  when a knob does. Only this row is rebuilt, for the same reason
 *  refreshIdle() exists: a full redraw under the reader's pointer would drop
 *  the control they are currently holding. */
function refreshPresets() {
  if (presetsBox === null) return;
  const next = drawPresets();
  if (next === null) return;
  presetsBox.replaceWith(next);
  presetsBox = next;
}

/** Re-state the knobs whose meaning depends on another setting, without
 *  rebuilding the surface — a redraw under the reader's pointer would drop
 *  the control they are holding. Same state() the draw uses, so the sentence
 *  beside a control is corrected rather than left standing. */
function refreshIdle() {
  for (const control of controls.values()) {
    if (!control.knob.needs) continue;
    state(control);
  }
}

function schedule() {
  clearTimeout(timer);
  if (hold === null) hold = holdEngine();
  timer = setTimeout(settle, SETTLE_MS);
}

/** The settle is over: what the moved knobs owe takes its turn on the
 *  script's book (book-queue.js). Serialised behind whatever already has
 *  the book: two conversions writing the same EPUB is a race, and a slow
 *  early one finishing last would leave the file disagreeing with the
 *  screen. When something else has it (a send, a copy, a conversion), the
 *  status line says so until the save starts and says "Saving". */
function settle() {
  timer = null;
  const mine = era;
  const book = bookOf(ctx.state.script);
  // A save rewrites the book, so it waits for every turn running on it.
  const ahead = holders(book);
  if (ahead.length > 0) say(statusFor('waiting', ahead));
  // Tied to the script it was queued for. Its turn can come long after
  // (behind a minute-long KFX send), and flush() reads the script on screen
  // and what it owes when it runs: by then another script's changes, whose
  // own settle saves them on their own book. scriptChanged() already dropped
  // what this one owed and let its hold go, so skipping it loses nothing.
  inTurn(book, 'save', () => (era === mine ? flush() : undefined));
}

/** Which book a save takes its turn on: the script's library EPUB, which
 *  its rebuild writes. A script with no EPUB still has settings to store,
 *  so its .fountain stands in and its saves still come one at a time. */
function bookOf(script) {
  return script?.epubPath ?? script?.fountainPath ?? '';
}

/** A settle still counting down is cut short, not waited out, whenever any
 *  page asks for a turn on a book: its save takes its turn first, so a send
 *  started 100 ms after a knob moved sends what the reader last set. */
beforeEveryTurn(() => {
  if (timer === null) return;
  clearTimeout(timer);
  settle();
});

/** Let the settle's hold go, if one is held. Safe to call any number of
 *  times: app.js's release is idempotent, and this forgets it after the
 *  first call either way. */
function releaseHold() {
  if (hold === null) return;
  const release = hold;
  hold = null;
  release();
}

async function flush() {
  const script = ctx.state.script;
  const mine = era;
  const owed = pending;
  pending = {};
  if (!isPending(owed) || !script?.fountainPath) {
    // Nothing to save after all (a later flush already carried it, or the
    // script went away): nothing for a restart to wait on either.
    releaseHold();
    return;
  }

  /** A change that was not stored is still owed. Clearing `pending` before
   *  the engine is asked is what makes a knob moved DURING the save land in
   *  the next one; but if the save itself fails, dropping what it carried
   *  would leave the screen showing values the engine never wrote. */
  const giveBack = () => { pending = restorePending(owed, pending); };
  /** Another script was opened while this was in flight. The engine has
   *  already been asked and there is no taking that back, but painting its
   *  answer would put the previous script's pages into the reader. */
  const stale = () => era !== mine;

  say(statusFor('saving'));
  try {
    const saving = runEngine(argv.settings(script.fountainPath, JSON.stringify(owed)));
    // The save's engine call is counted from the moment runEngine() is
    // called, before its first await, so the settle's hold can go now with
    // no gap between the two. Released before awaiting rather than after:
    // held through the save, it would count the same work twice.
    releaseHold();
    const saved = settingsFrom(await saving);
    if (stale()) return;
    if (saved === null) {
      giveBack();
      say(statusFor('failed', NO_MESSAGE));
      return;
    }
    script.settings = saved;
    // The engine is the authority on what was stored. It agrees with the
    // screen whenever the clamps agree, which is why this redraw is silent
    // almost always — and why, when it does not, the screen is the thing
    // that is wrong and gets redrawn rather than the file. A knob moved
    // while this was in flight is not yet in `saved`, so the screen keeps
    // what it has and the next flush reconciles.
    if (!isPending(pending) && !sameSettings(saved, settings)) {
      settings = saved;
      draw();
    }
    if (script.epubPath === null) {
      say(statusFor('saved'));
      return;
    }
    const answer = await runEngine(
      argv.reconvert(script.fountainPath, script.epubPath, JSON.stringify(script.settings)));
    if (stale()) return;
    if (answer.ok !== true) {
      // The settings ARE stored; it is the book that did not get rebuilt,
      // and the surface may not claim otherwise. Owing them again is what
      // makes the next change try the rebuild again.
      giveBack();
      say(statusFor('failed', answer.error?.message));
      return;
    }
    script.previewHtml = answer.previewHtml ?? script.previewHtml;
    // Both frames, because the reader is a surface the reader may come back
    // to and this one is on screen right now. They render the same document;
    // neither is a copy of the other's rendering.
    renderReader(script.previewHtml);
    renderPreview(script.previewHtml);
    say(statusFor(isPending(pending) ? 'pending' : 'saved'));
  } catch (err) {
    if (stale()) return;
    giveBack();
    say(statusFor('failed', err.message));
  }
}

function say(status) {
  if (statusLine === null) return;
  text(statusLine, status.line);
  statusLine.classList.toggle('bad', status.bad);
}
