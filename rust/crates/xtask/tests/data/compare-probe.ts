// Asks one comparator module (the path in argv[2]: src/update/compare.ts or
// desktop/ui/update-compare.js) every question tests/update_compare.rs
// asks, and prints the answers as one JSON object. Run with bun.
const m = await import(process.argv[2]!);

export const PAIRS: [string, string][] = [
  ['0.6.0', '0.6.0-1-g965cb10'],
  ['0.6.0', '0.6.0-1-g965cb10-dirty'],
  ['0.6.0', '0.6.0-dirty'],
  ['0.7.0', '0.6.0-1-g965cb10'],
  ['0.6.1', '0.6.0'],
  ['0.6.0', '0.6.0'],
  ['0.5.4', '0.6.0'],
  ['0.10.0', '0.9.0'],
  ['1.0.0', '0.99.99'],
  ['0.6.0', '0.6.0-beta.1'],
  ['0.6.0', '0.3.0-dev'],
  ['0.6.0-beta.10', '0.6.0-beta.9'],
  ['v0.4.0', '0.3.0'],
  ['0.3.0+ci.7', '0.3.0'],
];
const SUFFIXES = ['1-g965cb10', 'dirty', 'beta.1', 'dev', '1-g', 'x-g965cb10'];
const PICKS = [
  null,
  { version: '0.7.0', currentVersion: '0.6.0' },
  { version: '0.6.0', currentVersion: '0.6.0-1-g965cb10' },
  { version: 'not-a-version', currentVersion: '0.6.0' },
  { version: '0.6.0', currentVersion: '' },
];
const NOW = 1_800_000_000_000;
const H = 3600_000;
const CHECKS: [boolean, number | null][] = [
  [false, null],
  [true, null],
  [true, NOW - H],
  [true, NOW - 25 * H],
  [true, NOW + H],
];

console.log(
  JSON.stringify({
    isNewer: PAIRS.map(([a, b]) => m.isNewer(a, b)),
    normalizeVersion: PAIRS.map(([a]) => m.normalizeVersion(a)),
    isDescribeSuffix: SUFFIXES.map((s) => m.isDescribeSuffix(s)),
    pickUpdate: PICKS.map((c) => m.pickUpdate(c)),
    shouldCheck: CHECKS.map(([o, l]) => m.shouldCheck(o, l ?? undefined, NOW)),
    checkIntervalMs: m.CHECK_INTERVAL_MS,
  }),
);
