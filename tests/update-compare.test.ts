// The judgement half of the updater: is this release newer than the
// build that is running?
//
// Tauri's updater plugin answers that with semver and would be wrong in
// one specific, constant way. A build past a tag describes itself as
// "0.6.0-1-g965cb10", and semver reads a hyphen suffix as a PRE-release,
// so semver says that build is OLDER than 0.6.0 and offers 0.6.0 as an
// upgrade. It is not older. It is the tag plus one commit. Offering the
// tag installs a downgrade, every time, on every developer machine.
//
// The Swift app's UpdateCheck found that the hard way and guarded it. This
// is the port of the guard, and it is why the plugin does transport while the
// engine keeps the decision. See
// docs/superpowers/specs/2026-09-21-updater-design.md.
import { describe, test, expect } from 'bun:test';
import {
  normalizeVersion,
  isNewer,
  isDescribeSuffix,
  pickUpdate,
  shouldCheck,
  CHECK_INTERVAL_MS,
} from '../src/update/compare';

describe('normalizeVersion', () => {
  test('a leading v is dropped, either case', () => {
    // Tags are written v0.6.0 and the running app reports 0.6.0. They are
    // the same release and must compare equal.
    expect(normalizeVersion('v0.6.0')).toBe('0.6.0');
    expect(normalizeVersion('V0.6.0')).toBe('0.6.0');
    expect(normalizeVersion('0.6.0')).toBe('0.6.0');
  });

  test('build metadata after + is dropped, because semver says it is not precedence', () => {
    expect(normalizeVersion('0.6.0+ci.7')).toBe('0.6.0');
    expect(normalizeVersion('v0.6.0+ci.7')).toBe('0.6.0');
  });

  test('surrounding whitespace goes', () => {
    expect(normalizeVersion('  v0.6.0\n')).toBe('0.6.0');
  });

  test('a pre-release suffix is KEPT, because it does affect precedence', () => {
    expect(normalizeVersion('v0.6.0-beta.1')).toBe('0.6.0-beta.1');
  });
});

describe('isNewer: the ordinary cases', () => {
  test('a later patch, minor or major is newer', () => {
    expect(isNewer('0.6.1', '0.6.0')).toBe(true);
    expect(isNewer('0.7.0', '0.6.9')).toBe(true);
    expect(isNewer('1.0.0', '0.9.9')).toBe(true);
  });

  test('the same version is not newer than itself', () => {
    expect(isNewer('0.6.0', '0.6.0')).toBe(false);
    expect(isNewer('v0.6.0', '0.6.0')).toBe(false);
  });

  test('an older release is never newer', () => {
    expect(isNewer('0.5.4', '0.6.0')).toBe(false);
  });

  test('0.10.0 beats 0.9.0, which STRING ordering gets backwards', () => {
    // The classic way a version check quietly stops offering updates
    // after the tenth minor release. Named in the Swift UpdateCheck's own
    // comment as the reason it compares numerically per component.
    expect(isNewer('0.10.0', '0.9.0')).toBe(true);
    expect(isNewer('0.9.0', '0.10.0')).toBe(false);
    // And the same trap one level down.
    expect(isNewer('0.6.10', '0.6.9')).toBe(true);
  });

  test('missing components count as zero, so 1 and 1.0.0 are the same release', () => {
    expect(isNewer('1', '1.0.0')).toBe(false);
    expect(isNewer('1.0.1', '1')).toBe(true);
  });
});

describe('isNewer: pre-releases', () => {
  test('a real release beats a pre-release of the same numbers', () => {
    // So a running beta is offered the matching stable, which is wanted.
    expect(isNewer('0.6.0', '0.6.0-beta.1')).toBe(true);
  });

  test('a pre-release never beats the release of the same numbers', () => {
    expect(isNewer('0.6.0-beta.1', '0.6.0')).toBe(false);
  });

  test('two pre-releases compare numerically, so beta.10 beats beta.9', () => {
    expect(isNewer('0.6.0-beta.10', '0.6.0-beta.9')).toBe(true);
    expect(isNewer('0.6.0-beta.9', '0.6.0-beta.10')).toBe(false);
  });
});

describe('isDescribeSuffix: what a build past a tag looks like', () => {
  test('N-g<hex> is git describe output', () => {
    expect(isDescribeSuffix('1-g965cb10')).toBe(true);
    expect(isDescribeSuffix('42-gabcdef0')).toBe(true);
  });

  test('so is the same with -dirty, and bare dirty', () => {
    expect(isDescribeSuffix('1-g965cb10-dirty')).toBe(true);
    expect(isDescribeSuffix('dirty')).toBe(true);
  });

  test('an ordinary pre-release tag is NOT describe output', () => {
    // The whole guard rests on telling these apart. Read it too widely
    // and a real beta stops being offered its stable release.
    expect(isDescribeSuffix('beta.1')).toBe(false);
    expect(isDescribeSuffix('rc1')).toBe(false);
    expect(isDescribeSuffix('alpha')).toBe(false);
  });

  test('near misses are not describe output either', () => {
    expect(isDescribeSuffix('1-g')).toBe(false); // hash too short
    expect(isDescribeSuffix('1-gzzzzzz')).toBe(false); // not hex
    expect(isDescribeSuffix('x-g965cb10')).toBe(false); // count not numeric
    expect(isDescribeSuffix('-g965cb10')).toBe(false); // no count
    expect(isDescribeSuffix('g965cb10')).toBe(false); // no count at all
  });
});

describe('isNewer: the defence, which is the reason this file exists', () => {
  test('a tag is NOT an update for a build past that tag', () => {
    // 0.6.0-1-g965cb10 is 0.6.0 plus one commit. Semver reads the
    // hyphen as a pre-release and would call 0.6.0 an upgrade, which
    // installs a downgrade. This is the case that must never regress.
    expect(isNewer('0.6.0', '0.6.0-1-g965cb10')).toBe(false);
    expect(isNewer('0.6.0', '0.6.0-1-g965cb10-dirty')).toBe(false);
    expect(isNewer('0.6.0', '0.6.0-dirty')).toBe(false);
  });

  test('semver would disagree, and that disagreement is the point', () => {
    // Stated as an assertion so the file says out loud what it is
    // overruling. A plain pre-release in the same slot DOES get the
    // upgrade; only describe output does not.
    expect(isNewer('0.6.0', '0.6.0-beta.1')).toBe(true);
    expect(isNewer('0.6.0', '0.6.0-1-g965cb10')).toBe(false);
  });

  test('a genuinely later release IS still offered to a dev build', () => {
    // The guard must not wedge the updater shut. A dev build past 0.6.0
    // should still be offered 0.7.0.
    expect(isNewer('0.7.0', '0.6.0-1-g965cb10')).toBe(true);
    expect(isNewer('0.6.1', '0.6.0-1-g965cb10')).toBe(true);
  });

  test('a dev build is never an update for a released build', () => {
    expect(isNewer('0.6.0-1-g965cb10', '0.6.0')).toBe(false);
  });
});

describe('isNewer: the cases the Swift app pinned, one for one', () => {
  // These began as a mirror of the Swift app's own version-compare
  // checks, pair for pair, so the claim that those checks were REPLACED
  // was checkable rather than asserted (docs/retired-coverage.md,
  // `updater-version-compare`, cites the Swift lines; see them at tag
  // v0.7.4). Piece F3 deleted the Swift; the pairs stay, as ordinary
  // tests of src/update/compare.ts, with the same inputs and expectations.
  test('a dev build is offered the matching stable release', () => {
    // "-dev" is an ordinary pre-release, NOT describe output, so the
    // stable release does supersede it. This is the case that proves the
    // describe guard is narrow rather than a blanket "ignore suffixes".
    expect(isNewer('0.3.0', '0.3.0-dev')).toBe(true);
  });

  test('the remaining pairs the Swift app pinned', () => {
    expect(isNewer('v0.4.0', '0.3.0')).toBe(true);
    expect(isNewer('0.10.0', '0.9.0')).toBe(true);
    expect(isNewer('0.3.0', '0.3.0')).toBe(false);
    expect(isNewer('0.2.9', '0.3.0')).toBe(false);
    expect(isNewer('0.3.0-beta.1', '0.3.0')).toBe(false);
    expect(isNewer('1.0.0', '0.99.99')).toBe(true);
    expect(isNewer('0.3.0+ci.7', '0.3.0')).toBe(false);
    expect(isNewer('0.3.0', '0.3.0-1-g965cb10')).toBe(false);
    expect(isNewer('v0.3.0', '0.3.0-1-g965cb10-dirty')).toBe(false);
    expect(isNewer('0.3.0', '0.3.0-dirty')).toBe(false);
    expect(isNewer('0.3.1', '0.3.0-1-g965cb10')).toBe(true);
  });
});

describe('shouldCheck: opt-in and throttling, which are a privacy promise', () => {
  // The README says the update check is "off by default" and makes "at
  // most one request a day". Those are not preferences, they are claims
  // on a public page, and these are the five checks that hold them.
  // Mirrored, pair for pair, from the Swift app's own throttle checks
  // (tag v0.7.4); they outlived it at F3 unchanged.
  const now = 1_800_000_000_000; // ms, matching the Swift check's epoch
  const hours = (n: number) => n * 3600_000;

  test('never checks without opt-in, even on a first launch', () => {
    expect(shouldCheck(false, undefined, now)).toBe(false);
    // And not even when a check is long overdue.
    expect(shouldCheck(false, now - hours(48), now)).toBe(false);
  });

  test('the first opted-in launch checks', () => {
    expect(shouldCheck(true, undefined, now)).toBe(true);
  });

  test('an hour-old check is fresh enough', () => {
    expect(shouldCheck(true, now - hours(1), now)).toBe(false);
  });

  test('a day-old check re-checks', () => {
    expect(shouldCheck(true, now - hours(25), now)).toBe(true);
  });

  test('a clock set BACKWARDS does not trigger a check storm', () => {
    // lastChecked in the future makes the elapsed time negative. Without
    // care that reads as "overdue" and the app checks on every launch.
    expect(shouldCheck(true, now + hours(1), now)).toBe(false);
  });

  test('exactly at the interval checks, so the boundary is not a dead zone', () => {
    expect(shouldCheck(true, now - CHECK_INTERVAL_MS, now)).toBe(true);
    expect(shouldCheck(true, now - CHECK_INTERVAL_MS + 1, now)).toBe(false);
  });

  test('the interval is a day, stated once', () => {
    expect(CHECK_INTERVAL_MS).toBe(24 * 60 * 60 * 1000);
  });
});

describe('pickUpdate: what the window does with the plugin’s answer', () => {
  // The plugin's check() returns a candidate or null, having already
  // applied semver. This is the second opinion that overrules it.

  /** Assert a refusal and narrow to it, so `reason` can be read without
   *  a cast. A cast here would let an accidental `offer: true` past the
   *  typechecker and fail on an unrelated line. */
  const refusal = (d: ReturnType<typeof pickUpdate>): string => {
    if (d.offer) throw new Error(`expected a refusal, got an offer of ${d.version}`);
    return d.reason;
  };
  test('a newer release is offered', () => {
    expect(pickUpdate({ version: '0.7.0', currentVersion: '0.6.0' })).toEqual({
      offer: true,
      version: '0.7.0',
    });
  });

  test('null in, nothing offered', () => {
    expect(pickUpdate(null)).toEqual({ offer: false, reason: 'no update was reported' });
  });

  test('the plugin offering a downgrade to a dev build is REFUSED, with a reason', () => {
    // The whole integration, in one assertion: the plugin says yes by
    // semver, this says no, and it says why rather than silently
    // dropping it, because a silent drop is indistinguishable from a
    // broken update check.
    const reason = refusal(pickUpdate({ version: '0.6.0', currentVersion: '0.6.0-1-g965cb10' }));
    expect(reason).toMatch(/not newer/i);
    expect(reason).toMatch(/0\.6\.0-1-g965cb10/);
  });

  test('a malformed version is refused rather than guessed at', () => {
    // update-decoding's job: a bad release must not crash the check and
    // must not be installed either.
    expect(refusal(pickUpdate({ version: 'not-a-version', currentVersion: '0.6.0' }))).toMatch(
      /could not read/i,
    );
  });

  test('a missing currentVersion is refused, not treated as 0.0.0', () => {
    // Treating it as zero would make EVERY release an upgrade, which is
    // the failure that installs something over a newer build.
    expect(refusal(pickUpdate({ version: '0.6.0', currentVersion: '' }))).toMatch(
      /could not read/i,
    );
  });
});
