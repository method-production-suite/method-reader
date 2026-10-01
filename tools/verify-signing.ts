// Verify a PUBLISHED macOS DMG against the requirements the frozen Swift
// updater actually pins.
//
//   bun tools/verify-signing.ts --dmg <path> --expect coexist
//                            # v0.6.0 to 0.7.x: signed, and the old app must
//                            # REFUSE it (then named Screepub-Desktop-...)
//   bun tools/verify-signing.ts --dmg Screepub-macOS.dmg --expect handover
//                            # the identifier release: signed, and the old
//                            # app must TAKE it
//
// Step 3 of docs/superpowers/plans/2026-09-20-swift-to-tauri-handover.md.
// Signing has never executed: those secrets only reach a tagged release
// and no tag has ever built a Tauri app, so everything this repository
// says about the macOS signature is currently read off tauri-bundler's
// source rather than off an artifact. This is the tool that ends that, and
// it runs against a DOWNLOADED release asset, never against a local build,
// because the thing being tested is what CI produced and published.
//
// Why it is not the three commands the plan writes out. The plan's
// shorthand requirement is
//
//   =anchor apple generic and certificate leaf[subject.OU] = "XSRB3D643J"
//
// and that is strictly WEAKER than what the Swift app's installer pins: it
// drops both Developer ID certificate-chain clauses, and for the app it
// drops the bundle identifier. An artifact can satisfy the shorthand and
// still be refused by the installer, which is precisely the gap step 3
// exists to close. tests/verify-signing.test.ts pins the strings below
// literally, with the Swift that built them quoted beside the pin. The
// Swift source left this repository at piece F3; the installed apps that
// run it did not, so these strings are a contract with apps in the wild
// and this file is where it is written down.
//
// It also answers step 4 without anyone installing anything. The two
// releases differ in exactly one variable, the identifier, so "would the
// frozen updater install this?" is a question codesign can be asked
// directly: at v0.6.0 the answer must be NO and at v0.6.1 it must be YES.
// Finding out on a Mac with a working app on it is the expensive way.

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { mountDmgApp } from './dmg';
import { realRun, type RunResult, type Runner } from './smoke-cli';

/** UpdateInstaller.teamID. */
export const TEAM_ID = 'XSRB3D643J';
/** UpdateInstaller.bundleID: the identifier the Tauri app TAKES at v0.6.1. */
export const SWIFT_BUNDLE_ID = 'com.darkwell.screepub';

/** The two Developer ID chain clauses, shared by both requirements.
 *  `...6.2.6` is the Developer ID intermediate CA; `...6.1.13` is a
 *  Developer ID Application leaf. Together they are what stops an
 *  ad-hoc or self-signed artifact satisfying "anchor apple generic". */
export const CHAIN_CLAUSES = [
  ' and certificate 1[field.1.2.840.113635.100.6.2.6] exists',
  ' and certificate leaf[field.1.2.840.113635.100.6.1.13] exists',
] as const;

const TEAM_CLAUSE = ` and certificate leaf[subject.OU] = "${TEAM_ID}"`;

/** UpdateInstaller.appRequirement, for whichever identifier is being
 *  asked about. The parameter is the whole point: the frozen app always
 *  asks about SWIFT_BUNDLE_ID, and asking about the Tauri app's own
 *  identifier is how the chain is checked separately from the refusal. */
export function appRequirement(bundleID: string): string {
  return (
    `anchor apple generic and identifier "${bundleID}"` + CHAIN_CLAUSES.join('') + TEAM_CLAUSE
  );
}

/** UpdateInstaller.dmgRequirement. No identifier: a codesigned DMG's
 *  identifier is its filename stem, not a bundle id, so pinning one would
 *  fail every image no matter who signed it. */
export const DMG_REQUIREMENT = 'anchor apple generic' + CHAIN_CLAUSES.join('') + TEAM_CLAUSE;

/** Exactly UpdateInstaller.verify's invocation. The `=` prefix is
 *  load-bearing: without it codesign reads the argument as a PATH to a
 *  requirement file and fails for a reason that has nothing to do with
 *  the signature. */
export function codesignVerifyArgv(target: string, requirement: string): string[] {
  return ['codesign', '--verify', '--deep', '--strict', '--test-requirement', `=${requirement}`, target];
}

export function codesignDisplayArgv(target: string): string[] {
  return ['codesign', '-dv', '--verbose=4', target];
}

// ── notarization ─────────────────────────────────────────────────────
//
// A SEPARATE axis from everything above, and the reason this section
// exists: v0.6.0 shipped a DMG that passed every check in this file and
// was still wrong. tauri-bundler notarizes the .app (app.rs, right after
// signing it) and the DMG path only SIGNS (dmg/mod.rs, which links
// tauri-apps/tauri#12288 about not self-signing images). Nothing in the
// bundler ever submits the container, so the ticket ends up stapled to
// the app inside an image that carries none.
//
// The frozen updater does not care: dmgRequirement pins the anchor, the
// chain and the team, not notarization, which is exactly why this got
// through. A PERSON cares, because a browser download carries the
// quarantine attribute and Gatekeeper assesses the image when they
// double-click it. Measured on the published v0.6.0 artifacts, when
// Screepub-macOS.dmg was still the Swift app's image:
//
//   Screepub-macOS.dmg                     accepted, Notarized Developer ID
//   Screepub-Desktop-macOS-universal.dmg   rejected, Unnotarized Developer ID
//   the .app inside the latter              accepted, Notarized Developer ID
//
// So this was a regression against the DMG that shipped then, and the
// Swift app's release script was the reference: sign, notarize, staple,
// validate. release.yml's "Notarize and staple the disk image" step does
// the same to the window's image now.

/** `-t open`, not `-t exec`: the thing being assessed is a disk image a
 *  person double-clicks. The primary-signature context is what the Swift
 *  app's release script used too, and its comment recorded why: without it
 *  an unsigned-but-stapled image reports "no usable signature", a red flag
 *  that is not real. */
export function spctlAssessArgv(target: string): string[] {
  return ['spctl', '-a', '-t', 'open', '--context', 'context:primary-signature', '-v', target];
}

/** validate, never staple. This tool only reads; a verifier that repaired
 *  what it was checking could never fail. */
export function staplerValidateArgv(target: string): string[] {
  return ['xcrun', 'stapler', 'validate', target];
}

export type Notarization = 'notarized' | 'unnotarized' | 'unknown';

/** spctl's own word, read off `source=`.
 *
 *  Anchored, because the whole difference is one word: a contains-check
 *  for "Notarized" matches "Unnotarized" too and would report the broken
 *  artifact as fine, silently. Anything unrecognised is `unknown` rather
 *  than either answer, so a future spctl wording fails closed. */
export function parseNotarization(result: RunResult): Notarization {
  const text = `${result.stderr}\n${result.stdout}`;
  const source = /^source=(.+)$/m.exec(text)?.[1]?.trim();
  if (source === 'Notarized Developer ID') return 'notarized';
  if (source === 'Unnotarized Developer ID') return 'unnotarized';
  return 'unknown';
}

/** codesign -dv writes to STDERR. Reading only stdout reports "unsigned"
 *  for a perfectly signed bundle, so both are searched. */
export function parseIdentifier(result: RunResult): string | undefined {
  return /^Identifier=(.+)$/m.exec(`${result.stderr}\n${result.stdout}`)?.[1]?.trim();
}

export interface SigningVerdict {
  /** The container satisfies the requirement the installer checks first. */
  dmgSigned: boolean;
  /** A notarization ticket is attached to the IMAGE. Read off the file,
   *  so it answers the same offline as online. */
  dmgStapled: boolean;
  /** What the system says it would do with the image, which is a
   *  different question from whether a ticket is present. Both are kept
   *  because they can disagree, and a disagreement is not a pass. */
  dmgAssessment: Notarization;
  /** The .app satisfies the Developer ID chain and team, ignoring which
   *  identifier it carries. Separated from the identifier so a signing
   *  failure and a deliberate identifier difference cannot be confused. */
  appChainSigned: boolean;
  appIdentifier: string | undefined;
  /** The one that matters: the .app satisfies the FULL requirement the
   *  frozen UpdateInstaller pins, identifier included. */
  updaterWouldInstall: boolean;
}

export function judgeSigning(dmgPath: string, appPath: string, run: Runner): SigningVerdict {
  const passes = (target: string, requirement: string): boolean =>
    run(codesignVerifyArgv(target, requirement)).exitCode === 0;

  const appIdentifier = parseIdentifier(run(codesignDisplayArgv(appPath)));
  return {
    dmgSigned: passes(dmgPath, DMG_REQUIREMENT),
    dmgStapled: run(staplerValidateArgv(dmgPath)).exitCode === 0,
    dmgAssessment: parseNotarization(run(spctlAssessArgv(dmgPath))),
    // Asked about the app's OWN identifier, so this isolates the chain.
    appChainSigned: appIdentifier ? passes(appPath, appRequirement(appIdentifier)) : false,
    appIdentifier,
    updaterWouldInstall: passes(appPath, appRequirement(SWIFT_BUNDLE_ID)),
  };
}

/** Which release is being verified, and therefore what "correct" means.
 *  `coexist` is v0.6.0: prove signing while the identifier still differs,
 *  so nobody's app is touched. `handover` is v0.6.1: the identifier has
 *  been taken and existing installs are meant to upgrade themselves. */
export type Expectation = 'coexist' | 'handover';

export function describeVerdict(
  v: SigningVerdict,
  expectation: Expectation,
): { ok: boolean; lines: string[] } {
  const lines: string[] = [];
  let ok = true;
  const fail = (line: string): void => {
    ok = false;
    lines.push(`FAIL  ${line}`);
  };
  const pass = (line: string): void => {
    lines.push(`ok    ${line}`);
  };

  if (v.dmgSigned) pass(`the DMG satisfies the Developer ID chain and team ${TEAM_ID}`);
  else
    fail(
      `the DMG does NOT satisfy the requirement the installer checks first: ` +
        `Developer ID chain, team ${TEAM_ID}. Signing did not work. Stop here.`,
    );

  if (v.appChainSigned)
    pass(`the .app satisfies the Developer ID chain and team ${TEAM_ID}`);
  else
    fail(
      `the .app does NOT satisfy the Developer ID chain and team ${TEAM_ID}` +
        `${v.appIdentifier ? '' : ', and carries no identifier at all (is it signed?)'}. Stop here.`,
    );

  lines.push(`      the .app's identifier is ${v.appIdentifier ?? '<none>'}`);

  // Deliberately NOT scoped to an expectation. The frozen updater does
  // not check notarization, so an unnotarized image can satisfy every
  // line above and still stop a person who downloads it. There is no
  // release at which shipping that is right.
  //
  // Both signals must agree. stapler reads a ticket off the file; spctl
  // asks the system what it would do. They answer different questions
  // and normally agree, and if they ever do not, the image is in a state
  // nobody designed: guessing which to believe is how a bad DMG ships.
  if (v.dmgStapled && v.dmgAssessment === 'notarized') {
    pass('the DMG is notarized and carries its stapled ticket');
  } else if (!v.dmgStapled && v.dmgAssessment === 'unnotarized') {
    fail(
      `the DMG is NOT notarized: no stapled ticket, and Gatekeeper says ` +
        `"Unnotarized Developer ID". Signing is fine and the updater does not care, ` +
        `but a person who downloads this and double-clicks it meets Gatekeeper, and ` +
        `a release image must not do this to them. tauri-bundler notarizes the .app ` +
        `and never the image; release.yml's notarize step must notarize and staple the image too.`,
    );
  } else {
    fail(
      `the DMG's two notarization signals DISAGREE: stapler ${
        v.dmgStapled ? 'found a ticket' : 'found no ticket'
      }, Gatekeeper said "${v.dmgAssessment}". That is a state nobody designed, so it is ` +
        `not being read as a pass.`,
    );
  }

  if (expectation === 'coexist') {
    if (!v.updaterWouldInstall) {
      pass(
        `an installed Swift app would DOWNLOAD this and then refuse it: the identifier ` +
          `is not ${SWIFT_BUNDLE_ID}. That is the intended v0.6.0 behaviour, and nobody's ` +
          `app is replaced.`,
      );
    } else {
      fail(
        `an installed Swift app WOULD install this. At v0.6.0 the identifier is supposed ` +
          `to still differ, so the pin makes that impossible. Something has already taken ` +
          `${SWIFT_BUNDLE_ID} early. Do not ship: this is v0.6.1 behaviour arriving a ` +
          `release too soon, with signing unproven.`,
      );
    }
  } else {
    if (v.updaterWouldInstall) {
      pass(
        `an installed Swift app would download, verify and install this. That is the ` +
          `handover working.`,
      );
    } else {
      fail(
        `an installed Swift app would download this and then REFUSE it. At v0.6.1 the ` +
          `identifier must be ${SWIFT_BUNDLE_ID}; it is ${v.appIdentifier ?? '<none>'}. ` +
          `Every existing install would see a failing update, forever.`,
      );
    }
  }

  return { ok, lines };
}

if (import.meta.main) {
  const { values } = parseArgs({
    args: Bun.argv.slice(2),
    options: { dmg: { type: 'string' }, expect: { type: 'string' } },
    strict: true,
    allowPositionals: false,
  });
  const dmg = values.dmg;
  const expectation = values.expect;
  if (!dmg || (expectation !== 'coexist' && expectation !== 'handover')) {
    console.error(
      'verify-signing: --dmg <downloaded .dmg> --expect coexist|handover\n' +
        '  coexist  = v0.6.0: signed, and an installed Swift app must REFUSE it\n' +
        '  handover = v0.6.1: signed, and an installed Swift app must INSTALL it\n' +
        'Run it against the asset DOWNLOADED from the release, never a local build.',
    );
    process.exit(2);
  }
  if (process.platform !== 'darwin') {
    console.error(`verify-signing: needs codesign and hdiutil, which are macOS-only (this is ${process.platform}).`);
    process.exit(2);
  }
  const work = mkdtempSync(join(tmpdir(), 'screepub-verify-signing-'));
  let detach: (() => void) | undefined;
  // exitCode, never process.exit(), inside this try: exit() ends the process
  // on the spot, so the finally never ran and a failed check left the work
  // folder behind with the DMG still mounted inside it.
  try {
    let appPath: string;
    ({ appPath, detach } = mountDmgApp(dmg, work, realRun, 'verify-signing'));
    const verdict = judgeSigning(dmg, appPath, realRun);
    const { ok, lines } = describeVerdict(verdict, expectation);
    console.log(`verify-signing: ${dmg} (--expect ${expectation})`);
    for (const line of lines) console.log(`  ${line}`);
    if (ok) {
      console.log('\nverify-signing: all checks passed.');
    } else {
      console.error('\nverify-signing: the artifact does not match what this release is meant to be.');
      process.exitCode = 1;
    }
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exitCode = 1;
  } finally {
    detach?.();
    rmSync(work, { recursive: true, force: true });
  }
}
