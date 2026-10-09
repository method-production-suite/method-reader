// Is this release newer than the build that is running?
//
// A port (bf3ff32) of the Swift Mac app's UpdateCheck version comparison,
// and the ENGINE half
// of the updater. The Tauri plugin does transport: network, signature,
// download, swap. This decides, because deciding is what the governing
// ADR says belongs on this side of the line.
//
// The plugin answers the same question with semver, and semver is wrong
// in one constant way. A build past a tag describes itself as
// `0.6.0-1-g965cb10`, and semver reads any hyphen suffix as a
// PRE-release, so it calls that build older than `0.6.0` and offers
// `0.6.0` as an upgrade. It is not older; it is the tag plus one commit.
// Accepting the offer installs a downgrade, and it would do so on every
// developer machine, every time. The Swift's own comment (see it at tag
// v0.7.4; the Swift app was deleted at piece F3):
//
//   treating it as a pre-release would install a downgrade on every
//   post-tag dev build
//
// So the window asks the plugin what it found and then asks this whether
// to believe it. See docs/superpowers/specs/2026-09-21-updater-design.md.
//
// No network, no filesystem, no clock. Pure, so the whole of it is
// ordinary `bun test` coverage, which is the point: this replaced 17 of
// the Swift app's own checks that existed nowhere else
// (docs/retired-coverage.md, `updater-version-compare`).

/** Strip a leading `v` and any `+build` metadata, so `v0.6.0` and
 *  `0.6.0+ci.7` both compare as `0.6.0`. A pre-release suffix is KEPT,
 *  because unlike build metadata it does affect precedence. */
export function normalizeVersion(version: string): string {
  let v = version.trim();
  if (v.startsWith('v') || v.startsWith('V')) v = v.slice(1);
  const plus = v.indexOf('+');
  if (plus !== -1) v = v.slice(0, plus);
  return v;
}

/** git describe output after the tag: `N-g<hex>`, optionally `-dirty`,
 *  or a bare `dirty` for an at-tag build with local changes.
 *
 *  Read narrowly on purpose. Too wide and a real pre-release stops being
 *  offered its stable release, which is a silent failure in the opposite
 *  direction from the one this guards. */
export function isDescribeSuffix(pre: string): boolean {
  if (pre === 'dirty') return true;
  let body = pre;
  if (body.endsWith('-dirty')) body = body.slice(0, -'-dirty'.length);
  const dash = body.indexOf('-');
  if (dash <= 0) return false;
  const count = body.slice(0, dash);
  const hash = body.slice(dash + 1);
  if (!/^[0-9]+$/.test(count)) return false;
  // `g` then at least four hex digits: `g965cb10` yes, `g` alone no.
  return /^g[0-9a-f]{4,}$/i.test(hash);
}

/** -> [numeric components, pre-release tag or undefined] */
function parts(version: string): [number[], string | undefined] {
  const dash = version.indexOf('-');
  const core = dash === -1 ? version : version.slice(0, dash);
  const pre = dash === -1 ? undefined : version.slice(dash + 1);
  const nums = core.split('.').map((p) => Number.parseInt(p, 10));
  return [nums, pre];
}

/** True when a version string is one this comparator can reason about.
 *  Exported through pickUpdate rather than directly: a caller that needs
 *  to ask is a caller that should be using pickUpdate. */
function isReadable(version: string): boolean {
  const [nums] = parts(normalizeVersion(version));
  return nums.length > 0 && nums.every((n) => Number.isInteger(n) && n >= 0);
}

/** True when `candidate` is a strictly newer release than `current`.
 *
 *  Compares numerically per component, never as strings: string ordering
 *  puts `0.10.0` BEFORE `0.9.0`, which is how a version check quietly
 *  stops offering updates after the tenth minor release. */
export function isNewer(candidate: string, current: string): boolean {
  const [candNums, candPre] = parts(normalizeVersion(candidate));
  const [currNums, currPre] = parts(normalizeVersion(current));

  const width = Math.max(candNums.length, currNums.length);
  for (let i = 0; i < width; i++) {
    // A missing component is zero, so `1` and `1.0.0` are one release.
    const a = i < candNums.length ? candNums[i]! : 0;
    const b = i < currNums.length ? currNums[i]! : 0;
    if (a !== b) return a > b;
  }

  // Numerically equal. A release beats a pre-release of the same numbers,
  // and two pre-releases compare their tags.
  //
  // EXCEPT when what is running carries a git-describe suffix. That marks
  // a build AT or PAST the tag, so the tag is never an update for it.
  if (candPre === undefined && currPre === undefined) return false;
  if (candPre === undefined && currPre !== undefined) return !isDescribeSuffix(currPre);
  if (candPre !== undefined && currPre === undefined) return false;
  if (isDescribeSuffix(currPre!)) return false;
  // Numeric collation, so `beta.10` beats `beta.9`.
  return candPre!.localeCompare(currPre!, undefined, { numeric: true }) > 0;
}

/** What the plugin hands over: the release it found and the build that is
 *  running. `null` when it found nothing. */
export interface UpdateCandidate {
  version: string;
  currentVersion: string;
}

export type UpdateDecision =
  | { offer: true; version: string }
  | { offer: false; reason: string };

/** The second opinion over the plugin's semver answer.
 *
 *  Every refusal carries a REASON. A silent drop is indistinguishable
 *  from an update check that is simply broken, and the difference
 *  matters to whoever is holding the bug report. */
export function pickUpdate(candidate: UpdateCandidate | null): UpdateDecision {
  if (candidate === null) return { offer: false, reason: 'no update was reported' };

  const { version, currentVersion } = candidate;
  // A missing or unreadable current version must NOT be treated as
  // 0.0.0: that makes every release an upgrade, which is the failure
  // that installs something over a newer build.
  if (!isReadable(version) || !isReadable(currentVersion)) {
    return {
      offer: false,
      reason:
        `could not read the versions: offered ${JSON.stringify(version)}, ` +
        `running ${JSON.stringify(currentVersion)}`,
    };
  }

  if (!isNewer(version, currentVersion)) {
    return {
      offer: false,
      reason: `${version} is not newer than the running ${currentVersion}`,
    };
  }

  return { offer: true, version };
}

// ── when to ask at all ───────────────────────────────────────────────
//
// A port of UpdateCheck.shouldCheck, and it is a privacy promise rather
// than an optimisation. README.md says the update check is off by
// default and makes "at most once a day" one unauthenticated request.
// Those are claims on a public page, so they belong in tested code
// rather than in whatever the UI remembers to do.

/** One day. The Swift app's `checkInterval`, in milliseconds. */
export const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/** Whether to make a network request at all.
 *
 *  `lastChecked` and `now` are epoch milliseconds; `undefined` means
 *  never checked.
 *
 *  A clock set BACKWARDS makes the elapsed time negative, which is
 *  simply less than the interval, so it reads as fresh rather than as
 *  overdue. That is deliberate: the other reading turns a wrong clock
 *  into a request on every launch. */
export function shouldCheck(
  optedIn: boolean,
  lastChecked: number | undefined,
  now: number,
): boolean {
  // Opt-in is checked FIRST and alone. No amount of elapsed time makes a
  // request acceptable from someone who did not ask for it.
  if (!optedIn) return false;
  if (lastChecked === undefined) return true;
  return now - lastChecked >= CHECK_INTERVAL_MS;
}
