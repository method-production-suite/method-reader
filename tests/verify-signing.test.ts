// The gate at step 3 of docs/superpowers/plans/2026-09-20-swift-to-tauri-handover.md.
//
// Signing has NEVER executed. Those secrets only reach a tagged release and
// no tag has ever built a Tauri app, so every claim this repository makes
// about the macOS signature is read off tauri-bundler's source. Step 3 is
// where that stops being a reading and becomes a fact, and the plan is
// explicit that a green workflow is not the same fact as a signature that
// satisfies the requirement the frozen updater pins.
//
// Which is why the requirement is not retyped. The plan's own shorthand,
//
//   =anchor apple generic and certificate leaf[subject.OU] = "XSRB3D643J"
//
// is WEAKER than what the Swift app's installer actually pins: it drops
// both Developer ID certificate-chain checks, and for the app it drops the
// bundle identifier. A DMG could satisfy the shorthand and be refused by
// the real installer, which is the exact failure step 3 exists to catch.
// So the strings live in one place, tools/verify-signing.ts, and the first
// describe block below pins them.
//
// Until piece F3 that block read the Swift source out of this repository
// and compared it clause by clause. F3 deleted the source; it did not
// delete the installed apps, and every copy of the retired Swift app still
// on someone's Mac demands exactly these strings of anything it installs.
// So they are this repository's own definition now, pinned literally, with
// the Swift they came from quoted beside them as history.
import { describe, test, expect } from 'bun:test';
import {
  TEAM_ID,
  SWIFT_BUNDLE_ID,
  DMG_REQUIREMENT,
  CHAIN_CLAUSES,
  appRequirement,
  codesignVerifyArgv,
  codesignDisplayArgv,
  spctlAssessArgv,
  staplerValidateArgv,
  parseIdentifier,
  parseNotarization,
  judgeSigning,
  describeVerdict,
  type Expectation,
  type SigningVerdict,
} from '../tools/verify-signing';
import type { RunResult, Runner } from '../tools/smoke-cli';

const ok = (stdout = '', stderr = ''): RunResult => ({ exitCode: 0, stdout, stderr });
const bad = (stderr = 'not signed at all'): RunResult => ({ exitCode: 1, stdout: '', stderr });

describe('the pinned requirements are the retired app’s, not a paraphrase', () => {
  // What every installed copy of the Swift app runs, quoted from its
  // UpdateInstaller, unchanged in every Swift release from v0.4.0 on (see
  // it at tag v0.7.4 for the file as it last stood):
  //
  //   public static let teamID = "XSRB3D643J"
  //   public static let bundleID = "com.darkwell.screepub"
  //
  //   public static let appRequirement: String =
  //       "anchor apple generic and identifier \"\(bundleID)\""
  //           + " and certificate 1[field.1.2.840.113635.100.6.2.6] exists"
  //           + " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
  //           + " and certificate leaf[subject.OU] = \"\(teamID)\""
  //
  //   public static let dmgRequirement: String =
  //       "anchor apple generic"
  //           + " and certificate 1[field.1.2.840.113635.100.6.2.6] exists"
  //           + " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
  //           + " and certificate leaf[subject.OU] = \"\(teamID)\""
  //
  // and it verifies with `codesign --verify --deep --strict
  // --test-requirement "=\(requirement)"`. Those installs cannot be
  // updated to want anything else, so these literals are a contract with
  // apps in the wild: change one and the release gate starts checking a
  // requirement nobody's installer asks for.
  const APP_REQUIREMENT_AS_SHIPPED =
    'anchor apple generic and identifier "com.darkwell.screepub"' +
    ' and certificate 1[field.1.2.840.113635.100.6.2.6] exists' +
    ' and certificate leaf[field.1.2.840.113635.100.6.1.13] exists' +
    ' and certificate leaf[subject.OU] = "XSRB3D643J"';
  const DMG_REQUIREMENT_AS_SHIPPED =
    'anchor apple generic' +
    ' and certificate 1[field.1.2.840.113635.100.6.2.6] exists' +
    ' and certificate leaf[field.1.2.840.113635.100.6.1.13] exists' +
    ' and certificate leaf[subject.OU] = "XSRB3D643J"';

  test('the team ID and bundle ID are the installer’s own constants', () => {
    expect(TEAM_ID).toBe('XSRB3D643J');
    expect(SWIFT_BUNDLE_ID).toBe('com.darkwell.screepub');
  });

  test('appRequirement is, character for character, the one installed apps check', () => {
    expect(appRequirement(SWIFT_BUNDLE_ID)).toBe(APP_REQUIREMENT_AS_SHIPPED);
  });

  test('the DMG requirement is, character for character, the one installed apps check', () => {
    expect(DMG_REQUIREMENT).toBe(DMG_REQUIREMENT_AS_SHIPPED);
  });

  test('appRequirement is the DMG requirement plus exactly one identifier clause', () => {
    // Stated as a relationship rather than as a second literal, so the two
    // cannot drift apart: whatever the chain becomes, the app requirement
    // is that chain with an identifier bolted on.
    const req = appRequirement('com.example.thing');
    expect(req).toContain('identifier "com.example.thing"');
    for (const clause of CHAIN_CLAUSES) expect(req).toContain(clause);
    expect(req).toContain(TEAM_ID);
    expect(req.replace(' and identifier "com.example.thing"', '')).toBe(DMG_REQUIREMENT);
  });

  test('the DMG requirement pins the chain and the team but NOT an identifier', () => {
    // A codesigned DMG's identifier is its filename stem, not a bundle id.
    // Pinning one here would fail every DMG regardless of who signed it.
    expect(DMG_REQUIREMENT).not.toContain('identifier');
    expect(DMG_REQUIREMENT).toContain(TEAM_ID);
    for (const clause of CHAIN_CLAUSES) expect(DMG_REQUIREMENT).toContain(clause.trim());
  });

  test('the plan’s shorthand is a STRICT subset, which is why it is not used', () => {
    // Guards the reason this file exists. If the real requirement ever
    // became as weak as the shorthand, the extra checking here would be
    // theatre and this test says so out loud.
    const shorthand = `anchor apple generic and certificate leaf[subject.OU] = "${TEAM_ID}"`;
    expect(DMG_REQUIREMENT).not.toBe(shorthand);
    expect(DMG_REQUIREMENT.length).toBeGreaterThan(shorthand.length);
    expect(CHAIN_CLAUSES.length).toBe(2);
  });
});

describe('the codesign invocations, asserted element by element', () => {
  // Data, not a spawned process: only a Mac with a real signed artifact
  // could run these, and the argv is where the mistakes live.
  test('verify passes --deep --strict and an = -prefixed requirement', () => {
    expect(codesignVerifyArgv('/V/Screepub.app', 'anchor apple generic')).toEqual([
      'codesign',
      '--verify',
      '--deep',
      '--strict',
      '--test-requirement',
      '=anchor apple generic',
      '/V/Screepub.app',
    ]);
  });

  test('the = prefix is present, because without it codesign reads a FILE', () => {
    // `--test-requirement <text>` without the leading = is a path to a
    // requirement file. The frozen installer writes `"=\(requirement)"`
    // and so must this, or the check fails for the wrong reason.
    const argv = codesignVerifyArgv('/x.app', DMG_REQUIREMENT);
    expect(argv[5]!.startsWith('=')).toBe(true);
  });

  test('display asks for verbose=4, which is what prints Identifier=', () => {
    expect(codesignDisplayArgv('/V/Screepub.app')).toEqual([
      'codesign',
      '-dv',
      '--verbose=4',
      '/V/Screepub.app',
    ]);
  });

  test('spctl is asked the OPEN question, with the primary-signature context', () => {
    // -t open, not -t exec: the thing being assessed is a disk image a
    // person double-clicks, not an executable. The Swift app's release
    // script used exactly this invocation on its DMG for as long as it
    // shipped, and its comment recorded why the context matters -- without it an
    // unsigned-but-stapled image reports "no usable signature", which
    // reads as a red flag that is not real.
    expect(spctlAssessArgv('/tmp/x.dmg')).toEqual([
      'spctl',
      '-a',
      '-t',
      'open',
      '--context',
      'context:primary-signature',
      '-v',
      '/tmp/x.dmg',
    ]);
  });

  test('stapler is asked to VALIDATE, never to staple', () => {
    // This tool only ever reads. Stapling is the release workflow's job;
    // a verifier that repaired what it was checking could never fail.
    const argv = staplerValidateArgv('/tmp/x.dmg');
    expect(argv).toEqual(['xcrun', 'stapler', 'validate', '/tmp/x.dmg']);
    expect(argv).not.toContain('staple');
  });
});

describe('reading the identifier back off codesign', () => {
  // codesign -dv writes to STDERR, which is the detail that makes a naive
  // implementation report "no identifier" on a perfectly signed bundle.
  const SAMPLE = [
    'Executable=/Volumes/Screepub/Screepub.app/Contents/MacOS/Screepub',
    'Identifier=com.darkwell.screepub.desktop',
    'Format=app bundle with Mach-O universal (x86_64 arm64)',
    'TeamIdentifier=XSRB3D643J',
  ].join('\n');

  test('it is read from stderr', () => {
    expect(parseIdentifier(ok('', SAMPLE))).toBe('com.darkwell.screepub.desktop');
  });

  test('stdout is read too, so a future codesign that moves it still works', () => {
    expect(parseIdentifier(ok(SAMPLE, ''))).toBe('com.darkwell.screepub.desktop');
  });

  test('output with no Identifier line answers undefined rather than guessing', () => {
    expect(parseIdentifier(ok('', 'code object is not signed at all'))).toBeUndefined();
  });
});

describe('reading notarization back off spctl', () => {
  // Real output, captured from the v0.6.0 artifacts on 2026-09-21. Both
  // are on STDERR, like codesign's.
  const NOTARIZED = [
    '/tmp/Screepub-macOS.dmg: accepted',
    'source=Notarized Developer ID',
    'origin=Developer ID Application: Clockwork Post Production, LLC (XSRB3D643J)',
  ].join('\n');
  const UNNOTARIZED = [
    '/tmp/Screepub-macOS.dmg: rejected',
    'source=Unnotarized Developer ID',
  ].join('\n');

  test('a notarized image is read as notarized', () => {
    expect(parseNotarization(ok('', NOTARIZED))).toBe('notarized');
  });

  test('the v0.6.0 Tauri DMG’s actual output is read as unnotarized', () => {
    // Not a hypothetical: this is what shipped, and the string this
    // function has to recognise is the one that shipped with it.
    expect(parseNotarization({ exitCode: 3, stdout: '', stderr: UNNOTARIZED })).toBe('unnotarized');
  });

  test('"Notarized" is not matched by a loose search for "otarized"', () => {
    // The whole failure is one word long. A contains-check for
    // "Notarized" matches "Unnotarized" too, which would report the
    // broken artifact as fine, silently, forever.
    expect(parseNotarization(ok('', UNNOTARIZED))).not.toBe('notarized');
  });

  test('anything else answers unknown rather than guessing either way', () => {
    // A future spctl wording must not be read as a pass. Failing closed
    // here is the difference between a guard and a decoration.
    expect(parseNotarization(ok('', 'some future phrasing'))).toBe('unknown');
    expect(parseNotarization(bad(''))).toBe('unknown');
  });
});

describe('the verdict: what the frozen updater would do with this artifact', () => {
  /** A runner that answers by what is being asked, so a test states the
   *  artifact's real properties once and every codesign call agrees. */
  function artifact(opts: {
    dmgSigned: boolean;
    appSigned: boolean;
    identifier: string;
    /** Defaults to true, so every pre-existing case describes a CORRECT
     *  artifact and the notarization tests below are the ones that opt
     *  into the defect. */
    dmgNotarized?: boolean;
  }): Runner {
    const notarized = opts.dmgNotarized ?? true;
    return (argv) => {
      const target = argv[argv.length - 1]!;
      const isDmg = target.endsWith('.dmg');
      if (argv[0] === 'spctl') {
        return notarized
          ? ok('', `${target}: accepted\nsource=Notarized Developer ID\n`)
          : { exitCode: 3, stdout: '', stderr: `${target}: rejected\nsource=Unnotarized Developer ID\n` };
      }
      if (argv[0] === 'xcrun') {
        return notarized
          ? ok('The validate action worked!\n')
          : bad(`${target} does not have a ticket stapled to it.`);
      }
      if (argv[1] === '-dv') {
        return opts.appSigned || isDmg
          ? ok('', `Identifier=${isDmg ? 'Screepub-macOS' : opts.identifier}\n`)
          : bad();
      }
      const requirement = argv[5]!;
      if (isDmg) return opts.dmgSigned ? ok() : bad();
      if (!opts.appSigned) return bad();
      // The only thing that can fail a signed app here is the identifier
      // clause, which is exactly the variable the two releases differ on.
      const wants = /identifier "([^"]+)"/.exec(requirement)?.[1];
      return !wants || wants === opts.identifier ? ok() : bad('does not satisfy its designated Requirement');
    };
  }

  const v060 = () =>
    judgeSigning(
      '/tmp/Screepub-macOS.dmg',
      '/V/Screepub.app',
      artifact({ dmgSigned: true, appSigned: true, identifier: 'com.darkwell.screepub.desktop' }),
    );

  const v061 = () =>
    judgeSigning(
      '/tmp/Screepub-macOS.dmg',
      '/V/Screepub.app',
      artifact({ dmgSigned: true, appSigned: true, identifier: SWIFT_BUNDLE_ID }),
    );

  test('a correctly signed 0.6.0 artifact: signed, and the updater REFUSES it', () => {
    // The whole shape of the two-release plan, as one assertion. v0.6.0
    // proves signing while the identifier still differs, so an existing
    // Swift install downloads the DMG and then declines to install it.
    const v = v060();
    expect(v.dmgSigned).toBe(true);
    expect(v.appChainSigned).toBe(true);
    expect(v.appIdentifier).toBe('com.darkwell.screepub.desktop');
    expect(v.updaterWouldInstall).toBe(false);
  });

  test('a correctly signed 0.6.1 artifact: signed, and the updater ACCEPTS it', () => {
    const v = v061();
    expect(v.dmgSigned).toBe(true);
    expect(v.appChainSigned).toBe(true);
    expect(v.appIdentifier).toBe(SWIFT_BUNDLE_ID);
    expect(v.updaterWouldInstall).toBe(true);
  });

  test('an unsigned DMG is caught even when the app inside is fine', () => {
    // The container and the bundle are signed by separate steps and the
    // installer verifies both. Checking only the .app would pass an image
    // the updater refuses before it ever reaches the app.
    const v = judgeSigning(
      '/tmp/x.dmg',
      '/V/Screepub.app',
      artifact({ dmgSigned: false, appSigned: true, identifier: SWIFT_BUNDLE_ID }),
    );
    expect(v.dmgSigned).toBe(false);
    expect(v.updaterWouldInstall).toBe(true);
    // ... and the expectation below is what turns that into a failure.
  });

  test('the v0.6.0 defect: signed and stapled app, unnotarized IMAGE', () => {
    // The exact shape that shipped. Everything the frozen updater checks
    // passes, because dmgRequirement pins the chain and the team and not
    // notarization, so the artifact is simultaneously correct for the
    // updater and wrong for a person who downloads it in a browser.
    // Those are two different questions and the verdict answers both.
    const v = judgeSigning(
      '/tmp/Screepub-macOS.dmg',
      '/V/Screepub.app',
      artifact({
        dmgSigned: true,
        appSigned: true,
        identifier: 'com.darkwell.screepub.desktop',
        dmgNotarized: false,
      }),
    );
    expect(v.dmgSigned).toBe(true);
    expect(v.updaterWouldInstall).toBe(false);
    expect(v.dmgStapled).toBe(false);
    expect(v.dmgAssessment).toBe('unnotarized');
  });

  test('a correct artifact reports both notarization signals agreeing', () => {
    const v = v060();
    expect(v.dmgStapled).toBe(true);
    expect(v.dmgAssessment).toBe('notarized');
  });
});

describe('the expectation each release is held to', () => {
  // Typed as the interface, not inferred: inference narrows
  // dmgAssessment to the literal 'notarized' and every spread below that
  // overrides it then fails to typecheck.
  const signed060: SigningVerdict = {
    dmgSigned: true,
    dmgStapled: true,
    dmgAssessment: 'notarized',
    appChainSigned: true,
    appIdentifier: 'com.darkwell.screepub.desktop',
    updaterWouldInstall: false,
  };

  const judge = (v: SigningVerdict, e: Expectation) => describeVerdict(v, e);

  test('coexist passes when signing works and the identifier still differs', () => {
    const r = judge(signed060, 'coexist');
    expect(r.ok).toBe(true);
    expect(r.lines.join(' ')).toContain('refuse');
  });

  test('coexist FAILS if the updater would install it: that is step 4’s alarm', () => {
    // The plan: "If it OFFERS the Tauri build, stop and find out why -- the
    // pin is supposed to make that impossible." Caught here, before anyone
    // installs anything, rather than on a Mac with a working app on it.
    const r = judge({ ...signed060, appIdentifier: SWIFT_BUNDLE_ID, updaterWouldInstall: true }, 'coexist');
    expect(r.ok).toBe(false);
    expect(r.lines.join(' ')).toContain('v0.6.1');
  });

  test('coexist FAILS on an unsigned DMG, which is the point of the whole step', () => {
    const r = judge({ ...signed060, dmgSigned: false }, 'coexist');
    expect(r.ok).toBe(false);
  });

  test('coexist FAILS on an unsigned app even if the DMG is signed', () => {
    const r = judge({ ...signed060, appChainSigned: false }, 'coexist');
    expect(r.ok).toBe(false);
  });

  test('handover passes only when the updater would actually install it', () => {
    const taken = { ...signed060, appIdentifier: SWIFT_BUNDLE_ID, updaterWouldInstall: true };
    expect(judge(taken, 'handover').ok).toBe(true);
    expect(judge(signed060, 'handover').ok).toBe(false);
  });

  test('an unnotarized image FAILS, in BOTH modes', () => {
    // Not scoped to one release. The frozen updater does not check
    // notarization, so an unnotarized image can satisfy every other line
    // here and still make Gatekeeper object when a person double-clicks
    // the download. There is no release at which shipping that is right,
    // so there is no mode in which this passes.
    const broken = { ...signed060, dmgStapled: false, dmgAssessment: 'unnotarized' as const };
    expect(judge(broken, 'coexist').ok).toBe(false);
    expect(
      judge({ ...broken, appIdentifier: SWIFT_BUNDLE_ID, updaterWouldInstall: true }, 'handover').ok,
    ).toBe(false);
  });

  test('the notarization failure says it is about the PERSON, not the updater', () => {
    // A reader who sees "signature" twice will assume signing broke and
    // go looking in the wrong place. The message has to name the actual
    // consequence: Gatekeeper, a download, a double-click.
    const broken = { ...signed060, dmgStapled: false, dmgAssessment: 'unnotarized' as const };
    const text = judge(broken, 'coexist').lines.join('\n');
    expect(text).toMatch(/notariz/i);
    expect(text).toMatch(/Gatekeeper|download|double-click/i);
    // And it must not claim the updater is affected, because it is not.
    expect(judge(broken, 'coexist').lines.some((l) => l.startsWith('ok') && /refuse/.test(l))).toBe(
      true,
    );
  });

  test('a disagreement between the two signals fails rather than picking one', () => {
    // stapler reads a ticket off the file; spctl asks the system what it
    // would do. They answer different questions and normally agree. If
    // they ever do not, the artifact is in a state nobody designed and
    // guessing which one to believe is how a bad DMG ships.
    const staplerOnly = { ...signed060, dmgAssessment: 'unknown' as const };
    expect(judge(staplerOnly, 'coexist').ok).toBe(false);
    const spctlOnly = { ...signed060, dmgStapled: false };
    expect(judge(spctlOnly, 'coexist').ok).toBe(false);
  });

  test('every failure says which artifact and which requirement, never just "failed"', () => {
    const r = judge({ ...signed060, dmgSigned: false, appChainSigned: false }, 'coexist');
    expect(r.ok).toBe(false);
    const text = r.lines.join('\n');
    expect(text).toContain('DMG');
    expect(text).toContain('.app');
    expect(text).toContain(TEAM_ID);
  });
});
