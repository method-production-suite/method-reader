#!/usr/bin/env bun
/**
 * Prints the device-pass checklist for tests/fixtures/torture.pdf.
 *
 *   bun tools/device-checklist.ts
 *
 * Generated from tools/torture-manifest.json rather than hand-maintained, so
 * it cannot drift from the coverage the fixture actually has. Hand-keeping it
 * would guarantee a second, stale copy of the coverage list, which is the
 * failure the manifest exists to prevent.
 */
import { readFileSync } from 'node:fs';

interface Row {
  entry: string;
  title: string;
  covered: boolean;
  page?: number;
  side?: string;
  how?: string;
  why?: string;
  /** Questions about a covered entry that only a person holding the device
   * can answer, one per line of the checklist. The picture entries carry
   * them: their `how` says what the fixture draws, and these say what to
   * look for on the page. */
  deviceChecks?: string[];
}

const manifest: Row[] = JSON.parse(readFileSync('tools/torture-manifest.json', 'utf8'));

const device = manifest
  .filter((r) => r.covered && (r.side === 'device' || r.side === 'both'))
  .sort((a, b) => (a.page ?? 0) - (b.page ?? 0) || a.entry.localeCompare(b.entry));

const uncovered = manifest.filter((r) => !r.covered);

const checked = manifest.filter((r) => (r.deviceChecks ?? []).length > 0);
// The picture entries (#19, #19a, #19b) and Screepub's own AZW3 (#20) each
// get a section of their own: different questions about the same book.
const pictures = checked.filter((r) => r.entry.startsWith('19'));
const azw3 = checked.filter((r) => r.entry === '20');
const pictureChecks = pictures.reduce((n, r) => n + (r.deviceChecks ?? []).length, 0);
const azw3Checks = azw3.reduce((n, r) => n + (r.deviceChecks ?? []).length, 0);

const out: string[] = [];
out.push('# Device pass: THE PROOF SHEET');
out.push('');
out.push('Convert `tests/fixtures/torture.pdf` and sideload it over USB into');
out.push("the Kindle's `documents/` folder.");
out.push('');
out.push('**Use KFX.** AZW3 and MOBI ignore widows/orphans entirely (registry');
out.push('17), so on those formats the split-minimums check below would pass');
out.push('without proving anything.');
out.push('');
out.push('**Read it at two font sizes at least.** Page breaks move with font');
out.push('size, so a keep that holds at one size can fail at another. Changing');
out.push('size re-rolls every break point and turns one file into two trials.');
out.push('');
out.push('Speeches name their own numbers, so a defect reports as "speech');
out.push('twenty-seven, one line alone at the bottom" and can be found again.');
out.push('');

for (const row of device) {
  out.push(`- [ ] **#${row.entry}, ${row.title}** (sheet ${row.page})`);
  out.push(`      ${row.how}`);
  out.push('');
}

out.push('## Not covered by this fixture');
out.push('');
for (const row of uncovered) {
  out.push(`- **#${row.entry}, ${row.title}.** ${row.why}`);
}
out.push('');

if (pictures.length > 0) {
  out.push('## The pictures');
  out.push('');
  out.push('torture.pdf draws its own pictures, so these are checks on the same');
  out.push('book as everything above, except two. The picture size an older');
  out.push('Kindle accepts in a .mobi has four small books of its own, built with');
  out.push('`bun tools/mobi-picture-probe.ts --out <folder>`. And the default book');
  out.push('makes its title card the cover (#19b), so the check on a full-page');
  out.push('plate at the front reads a second build with the cover off, whose');
  out.push('command that check gives. What each of torture.pdf\'s pictures is, and');
  out.push('which sheet it is on, is in the #19, #19a and #19b entries above.');
  out.push('');
  out.push('Read them in the KFX or the AZW3. The .mobi, the file for the oldest');
  out.push('Kindles, is text-only for now (MOBI_CARRIES_PICTURES in');
  out.push('src/mobi/html.ts), until the probe books above open with every');
  out.push('picture on an older Kindle; its rows below say what to check either');
  out.push('way. Once it carries them, it has no way to give a picture a share');
  out.push('of the text column, so the device sizes each one there: a width');
  out.push('unlike the KFX\'s is not a finding in the .mobi.');
  out.push('');
  out.push('Dark mode has nothing to show here: none of these pictures has');
  out.push('transparency, so each is a solid rectangle on a dark page as on a');
  out.push('light one. How a reader composites a transparent picture onto a dark');
  out.push('page needs a script that has one.');
  out.push('');
  for (const row of pictures) {
    out.push(`### #${row.entry}, ${row.title}`);
    out.push('');
    for (const check of row.deviceChecks ?? []) {
      out.push(`- [ ] ${check}`);
      out.push('');
    }
  }
}

if (azw3.length > 0) {
  out.push("## Screepub's own AZW3 (beta)");
  out.push('');
  out.push('The same book as an AZW3 that Screepub writes itself, with no');
  out.push('Calibre: what a Kindle gets over USB wherever KFX is not set up. It');
  out.push('has passed Calibre\'s own reader in the tests and has never been on a');
  out.push('Kindle, so these come first on a device pass. The command that builds');
  out.push('it is in the #20 entry above.');
  out.push('');
  for (const row of azw3) {
    for (const check of row.deviceChecks ?? []) {
      out.push(`- [ ] ${check}`);
      out.push('');
    }
  }
}

out.push(`${device.length} device-side entries, ${uncovered.length} declared gaps, `
  + `${pictureChecks} picture checks, ${azw3Checks} AZW3 checks.`);
out.push('');
out.push('Findings go in the named entry\'s `Device verdict:` slot in');
out.push('`docs/formatting-options-log.md`, in your own words.');

console.log(out.join('\n'));
