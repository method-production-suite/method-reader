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

const pictures = manifest.filter((r) => (r.deviceChecks ?? []).length > 0);
const pictureChecks = pictures.reduce((n, r) => n + (r.deviceChecks ?? []).length, 0);

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
  out.push('book as everything above, except the last. The picture size an older');
  out.push('Kindle accepts in a .mobi has four small books of its own, built with');
  out.push('`bun tools/mobi-picture-probe.ts --out <folder>`. What each of');
  out.push('torture.pdf\'s pictures is, and which sheet it is on, is in the #19');
  out.push('and #19a entries above.');
  out.push('');
  out.push('Read them in the KFX or the AZW3, and in the .mobi Screepub builds');
  out.push('itself when Calibre is absent, which carries them too. The .mobi has');
  out.push('no way to give a picture a share of the text column, so the device');
  out.push('sizes each one there: a width unlike the KFX\'s is not a finding in');
  out.push('the .mobi.');
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

out.push(`${device.length} device-side entries, ${uncovered.length} declared gaps, `
  + `${pictureChecks} picture checks.`);
out.push('');
out.push('Findings go in the named entry\'s `Device verdict:` slot in');
out.push('`docs/formatting-options-log.md`, in your own words.');

console.log(out.join('\n'));
