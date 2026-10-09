// tools/build-update-manifest.ts: latest.json, built from what ARRIVED.
//
// The updater plugin fetches one static JSON file and reads, for the
// platform it is running on, a url and the CONTENT of the artifact's
// .sig file. This tool writes that file in release.yml's app-upload job,
// over the directory the bundle legs' artifacts were downloaded into, so
// the manifest describes the files that are actually published beside
// it -- the same rule SHA256SUMS-app already follows, for the same
// reason: no leg ever saw the others.
//
// It is data-driven off UPDATER_KINDS in build-app-bundle.ts, so the
// published name and the platform keys have one definition. A .sig whose
// artifact no row recognises is an ERROR, not a skip: when Linux or
// Windows signing is switched on, the manifest refuses to go out until
// the row that maps them exists.
import { describe, test, expect, afterAll } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  MANIFEST_NAME,
  RELEASE_REPO,
  assetUrl,
  collectSignedArtifacts,
  manifestEndpoint,
  parseManifestArgs,
  renderManifest,
  type SignedArtifact,
} from '../tools/build-update-manifest';
import { fakeSignatureBox } from './signature-box';

const ROOT = new URL('..', import.meta.url).pathname;
const OUT = mkdtempSync(join(tmpdir(), 'screepub-manifest-'));
afterAll(() => rmSync(OUT, { recursive: true, force: true }));

const TAR = 'Screepub-macOS.app.tar.gz';

/** A directory shaped like app-upload's `arrivals` after download-artifact:
 *  every installer, the checksums file, and (when signed) the archive and
 *  its signature. */
function arrivals(tag: string, opts: { sig?: boolean | string; tar?: boolean; extra?: Record<string, string> } = {}) {
  const dir = join(OUT, tag);
  mkdirSync(dir, { recursive: true });
  for (const n of [
    'Screepub_0.6.0_amd64.deb',
    'Screepub-0.6.0-1.x86_64.rpm',
    'Screepub-macOS.dmg',
    'Screepub-0.6.0-setup.exe',
    'SHA256SUMS-app',
  ]) {
    writeFileSync(join(dir, n), `contents of ${n}`);
  }
  if (opts.tar !== false) writeFileSync(join(dir, TAR), 'gzip bytes');
  if (opts.sig === undefined || opts.sig === true) {
    writeFileSync(join(dir, `${TAR}.sig`), fakeSignatureBox('Screepub.app.tar.gz'));
  } else if (typeof opts.sig === 'string') {
    writeFileSync(join(dir, `${TAR}.sig`), opts.sig);
  }
  for (const [n, c] of Object.entries(opts.extra ?? {})) writeFileSync(join(dir, n), c);
  return dir;
}

describe('the addresses', () => {
  test('the endpoint is the one URL the app is configured with', () => {
    // Two files must agree on this string: tauri.conf.json (what the app
    // asks) and this tool (where the file goes). This is the test that
    // makes them one definition rather than two that happen to match.
    const conf = JSON.parse(
      readFileSync(join(ROOT, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8'),
    ) as { plugins: { updater: { endpoints: string[] } } };
    expect(conf.plugins.updater.endpoints).toEqual([manifestEndpoint()]);
    expect(manifestEndpoint()).toBe(
      'https://github.com/ssandweiss/screepub/releases/latest/download/latest.json',
    );
    expect(RELEASE_REPO).toBe('ssandweiss/screepub');
    expect(MANIFEST_NAME).toBe('latest.json');
  });

  test('an asset url names the tag and the published filename, over TLS', () => {
    expect(assetUrl('ssandweiss/screepub', '0.6.1', TAR)).toBe(
      `https://github.com/ssandweiss/screepub/releases/download/v0.6.1/${TAR}`,
    );
  });
});

describe('collecting what arrived', () => {
  test('the signed archive becomes one artifact for Apple silicon, with its signature verbatim', () => {
    // darwin-aarch64 alone since the release after 0.9.0:
    // the Mac build is Apple silicon only, so the archive no longer answers
    // for darwin-x86_64 (plan 2026-10-01-f2-handover-amendment.md).
    const dir = arrivals('ok');
    const found = collectSignedArtifacts(dir);
    expect(found).toEqual([
      {
        name: TAR,
        signature: fakeSignatureBox('Screepub.app.tar.gz'),
        platforms: ['darwin-aarch64'],
      },
    ]);
  });

  test('an Intel archive arriving is an error, not a darwin-x86_64 row', () => {
    // The universal era's per-arch names map to nothing now, so a stray
    // x64 archive stops the manifest instead of being offered to Intel Macs.
    const dir = arrivals('intel', {
      extra: {
        'Screepub-macOS-x64.app.tar.gz': 'gzip bytes',
        'Screepub-macOS-x64.app.tar.gz.sig': fakeSignatureBox('Screepub-macOS-x64.app.tar.gz'),
      },
    });
    expect(() => collectSignedArtifacts(dir)).toThrow(/Screepub-macOS-x64\.app\.tar\.gz/);
  });

  test('installers and checksums are not artifacts, because nothing signed them', () => {
    const dir = arrivals('unsigned', { sig: false, tar: false });
    expect(collectSignedArtifacts(dir)).toEqual([]);
  });

  test('a signature with no artifact beside it is an error that names both', () => {
    const dir = arrivals('orphan', { tar: false });
    expect(() => collectSignedArtifacts(dir)).toThrow(new RegExp(`${TAR}\\.sig`));
    expect(() => collectSignedArtifacts(dir)).toThrow(/no such artifact|not beside|missing/i);
  });

  test('a signature for an artifact no row knows is an error, not a skip', () => {
    // The tripwire for the day Linux or Windows signing is switched on:
    // a .deb.sig arriving with no row to map it must stop the manifest,
    // or a signed installer ships that no platform can ever be offered.
    const dir = arrivals('unknown', {
      extra: { 'Screepub_0.6.0_amd64.deb.sig': fakeSignatureBox('Screepub_0.6.0_amd64.deb') },
    });
    expect(() => collectSignedArtifacts(dir)).toThrow(/Screepub_0\.6\.0_amd64\.deb/);
    expect(() => collectSignedArtifacts(dir)).toThrow(/UPDATER_KINDS|no row|platform/i);
  });

  test('a signature that is not a minisign box is refused, naming the file', () => {
    const dir = arrivals('badsig', { sig: '<html>Not Found</html>' });
    expect(() => collectSignedArtifacts(dir)).toThrow(new RegExp(`${TAR}\\.sig`));
  });

  test('a missing directory is a clear message', () => {
    expect(() => collectSignedArtifacts(join(OUT, 'nowhere'))).toThrow(/nowhere/);
  });
});

describe('rendering the manifest', () => {
  const one: SignedArtifact = {
    name: TAR,
    signature: fakeSignatureBox('Screepub.app.tar.gz'),
    platforms: ['darwin-aarch64'],
  };
  const when = new Date('2026-09-21T18:00:00Z');

  test('it is the shape the plugin reads: version, notes, pub_date, platforms', () => {
    const m = renderManifest('0.6.1', [one], { now: when });
    expect(m.version).toBe('0.6.1');
    expect(m.pub_date).toBe('2026-09-21T18:00:00.000Z');
    expect(Object.keys(m.platforms)).toEqual(['darwin-aarch64']);
    expect(m.platforms['darwin-aarch64']).toEqual({
      url: `https://github.com/ssandweiss/screepub/releases/download/v0.6.1/${TAR}`,
      signature: one.signature,
    });
  });

  test('the notes point a person at the release page for that version', () => {
    const m = renderManifest('0.6.1', [one], { now: when });
    expect(m.notes).toContain('https://github.com/ssandweiss/screepub/releases/tag/v0.6.1');
    expect(m.notes).toContain('0.6.1');
  });

  test('the version is the plain tag version, whatever it was handed', () => {
    // The plugin parses it with semver; a leading v would make every
    // release unparseable and every check fail.
    expect(renderManifest('v0.6.1', [one], { now: when }).version).toBe('0.6.1');
    expect(() => renderManifest('main', [one], { now: when })).toThrow(/MAJOR\.MINOR\.PATCH/);
  });

  test('a prerelease version is refused: releases/latest never points at one', () => {
    // GitHub's /releases/latest resolves to the newest NON-prerelease, so
    // a manifest for a prerelease would be published somewhere the
    // endpoint never reads, and would be wrong if it ever were.
    expect(() => renderManifest('0.7.0-rc1', [one], { now: when })).toThrow(/prerelease/i);
  });

  test('no artifacts is an error: an empty manifest is a stale-forever endpoint', () => {
    expect(() => renderManifest('0.6.1', [], { now: when })).toThrow(/no signed/i);
  });

  test('a repeated key is an error', () => {
    // Two archives both claiming darwin-aarch64: whichever won would be
    // silent, so neither may.
    const other: SignedArtifact = { name: 'Screepub-other.app.tar.gz', signature: 'a', platforms: ['darwin-aarch64'] };
    expect(() => renderManifest('0.6.1', [one, other], { now: when })).toThrow(/darwin-aarch64/);
  });

  test('the repository can be overridden, for a fork or a dry run', () => {
    const m = renderManifest('0.6.1', [one], { now: when, repo: 'someone/fork' });
    expect(m.platforms['darwin-aarch64']!.url).toContain('someone/fork');
    expect(m.notes).toContain('someone/fork');
  });
});

describe('the arguments', () => {
  test('--dir, --version and --out are required', () => {
    expect(() => parseManifestArgs(['--version', '0.6.1', '--out', 'x'])).toThrow(/--dir/);
    expect(() => parseManifestArgs(['--dir', OUT, '--out', 'x'])).toThrow(/--version/);
    expect(() => parseManifestArgs(['--dir', OUT, '--version', '0.6.1'])).toThrow(/--out/);
  });

  test('--repo defaults to this repository', () => {
    expect(parseManifestArgs(['--dir', OUT, '--version', 'v0.6.1', '--out', 'x'])).toMatchObject({
      version: 'v0.6.1',
      repo: RELEASE_REPO,
    });
  });
});

describe('end to end, through the real tool', () => {
  const run = async (args: string[]) => {
    const proc = Bun.spawn(['bun', join(ROOT, 'tools', 'build-update-manifest.ts'), ...args], {
      stdout: 'pipe',
      stderr: 'pipe',
    });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    return { stdout, stderr, exitCode };
  };

  test('it writes latest.json describing the signed archive that arrived', async () => {
    const dir = arrivals('e2e');
    const out = join(dir, MANIFEST_NAME);
    const r = await run(['--dir', dir, '--version', 'v0.6.1', '--out', out]);
    expect(r.exitCode).toBe(0);
    const m = JSON.parse(readFileSync(out, 'utf8'));
    expect(m.version).toBe('0.6.1');
    // Apple silicon only. An Intel Mac running an earlier universal build
    // looks for darwin-x86_64, finds nothing, and its check() rejects; the
    // window says why in plain words (desktop/ui/update.js).
    expect(Object.keys(m.platforms)).toEqual(['darwin-aarch64']);
    expect(m.platforms['darwin-x86_64']).toBeUndefined();
    expect(m.platforms['darwin-aarch64'].signature).toBe(
      fakeSignatureBox('Screepub.app.tar.gz'),
    );
    expect(m.platforms['darwin-aarch64'].url).toBe(
      `https://github.com/ssandweiss/screepub/releases/download/v0.6.1/${TAR}`,
    );
    // Said out loud, because a manifest that went out naming the wrong
    // platforms is a bug report from a stranger.
    expect(r.stdout).toContain('darwin-aarch64');
    expect(r.stdout).toContain(TAR);
  });

  test('nothing signed is a non-zero exit and no file', async () => {
    const dir = arrivals('e2e-empty', { sig: false, tar: false });
    const out = join(dir, MANIFEST_NAME);
    const r = await run(['--dir', dir, '--version', '0.6.1', '--out', out]);
    expect(r.exitCode).not.toBe(0);
    expect(r.stderr).toMatch(/no signed/i);
    expect(() => readFileSync(out)).toThrow();
  });
});
