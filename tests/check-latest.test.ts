// tools/check-latest.ts: does the PUBLISHED latest.json describe the
// newest release?
//
// The Homebrew tap served 0.3.0 for five releases because nothing compared
// the published file to the newest release. This is the same alarm for the
// updater's manifest, and it reads the manifest from the EXACT url the app
// reads it from, never from a checkout or a build directory: a file on a
// laptop is not evidence of what a user's app will fetch.
//
// Network is injected. Every case below is a table of urls and answers,
// so the judgement can be exercised against the failures that matter
// without a release existing.
import { describe, test, expect } from 'bun:test';
import { checkLatest, judgeManifest, type Fetcher, type Release } from '../tools/check-latest';
import { FAKE_KEY_ID, fakePublicKeyBox, fakeSignatureBox } from './signature-box';

const REPO = 'ssandweiss/screepub';
const TAR = 'Screepub-macOS.app.tar.gz';
/** The Intel command-line tarball: carried by the identifier release and
 *  every release before it, by none after. */
const INTEL_CLI = 'screepub-cli-macos-x64.tar.gz';
const url = (v: string, name: string) => `https://github.com/${REPO}/releases/download/v${v}/${name}`;
const SIG = fakeSignatureBox('Screepub.app.tar.gz');
const PUBKEY = fakePublicKeyBox();

const release = (v: string, assets: string[] = [TAR, `${TAR}.sig`, 'latest.json', 'Screepub-macOS.dmg']): Release => ({
  tagName: `v${v}`,
  assets: assets.map((name) => ({ name, url: url(v, name) })),
});

const manifest = (v: string, over: Partial<Record<string, { url: string; signature: string }>> = {}) =>
  JSON.stringify({
    version: v,
    notes: 'x',
    pub_date: '2026-09-21T18:00:00.000Z',
    platforms: {
      'darwin-aarch64': { url: url(v, TAR), signature: SIG },
      ...over,
    },
  });

describe('judging a manifest against a release', () => {
  test('a manifest that names the newest release, its assets and their signatures is fine', () => {
    const v = judgeManifest(manifest('0.6.1'), release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(v).toEqual({ ok: true, version: '0.6.1', platforms: ['darwin-aarch64'] });
  });

  test('a manifest a version behind is the tap failure, and says both versions', () => {
    const v = judgeManifest(manifest('0.6.1'), release('0.6.2'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toMatch(/0\.6\.1[\s\S]*0\.6\.2|0\.6\.2[\s\S]*0\.6\.1/);
  });

  test('a manifest that is not JSON, or has no platforms, is a problem rather than a crash', () => {
    const notJson = judgeManifest('<html>', release('0.6.1'), {}, PUBKEY);
    expect(notJson.ok).toBe(false);
    const empty = judgeManifest(JSON.stringify({ version: '0.6.1', platforms: {} }), release('0.6.1'), {}, PUBKEY);
    expect(empty.ok).toBe(false);
    if (!empty.ok) expect(empty.problems.join('\n')).toMatch(/platform/i);
  });

  test('a url that is not an asset of that release is a problem naming the url', () => {
    const wrong = url('0.6.0', TAR);
    const v = judgeManifest(
      manifest('0.6.1', { 'darwin-aarch64': { url: wrong, signature: SIG } }),
      release('0.6.1'),
      { [`${TAR}.sig`]: SIG },
      PUBKEY,
    );
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toContain(wrong);
  });

  test('a signature that differs from the published .sig is a problem', () => {
    // The plugin verifies the download against the manifest's signature.
    // If that text drifted from the .sig beside the archive, every install
    // fails, and only a user would find out.
    const other = fakeSignatureBox('Screepub.app.tar.gz', Buffer.from('1112131415161718', 'hex'));
    const v = judgeManifest(manifest('0.6.1'), release('0.6.1'), { [`${TAR}.sig`]: other }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toMatch(/signature/i);
  });

  test('a release with no .sig beside the archive is a problem', () => {
    const v = judgeManifest(manifest('0.6.1'), release('0.6.1', [TAR, 'latest.json']), {}, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toContain(`${TAR}.sig`);
  });

  test('a signature made by a key the app does not trust is a problem naming both key ids', () => {
    // tauri-cli only WARNS when the signing key and the configured public
    // key differ. A release signed with a rotated or wrong key would
    // publish fine and fail every install.
    const otherKey = Buffer.from('1112131415161718', 'hex');
    const foreign = fakeSignatureBox('Screepub.app.tar.gz', otherKey);
    const v = judgeManifest(manifest('0.6.1', {
      'darwin-aarch64': { url: url('0.6.1', TAR), signature: foreign },
    }), release('0.6.1'), { [`${TAR}.sig`]: foreign }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) {
      const text = v.problems.join('\n');
      expect(text).toContain(FAKE_KEY_ID.readBigUInt64LE(0).toString(16).toUpperCase());
      expect(text).toContain(otherKey.readBigUInt64LE(0).toString(16).toUpperCase());
    }
  });

  test('an empty public key in the config is a problem: the app trusts nothing yet', () => {
    const v = judgeManifest(manifest('0.6.1'), release('0.6.1'), { [`${TAR}.sig`]: SIG }, '');
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toMatch(/public key/i);
  });

  test('a manifest missing darwin-aarch64 is a problem: the Mac build serves it', () => {
    const intelOnly = JSON.stringify({
      version: '0.6.1',
      platforms: { 'darwin-x86_64': { url: url('0.6.1', TAR), signature: SIG } },
    });
    const v = judgeManifest(intelOnly, release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toContain('darwin-aarch64');
  });

  test('darwin-x86_64 is no longer REQUIRED, and a universal-era manifest still passes', () => {
    // The Mac build is Apple silicon only from the release after
    // 0.9.0, so its manifest has no darwin-x86_64 and must not
    // be called broken. The identifier release and older stay green: this
    // runs weekly against whatever release is newest, and until the next
    // one ships that is the identifier release, whose UNIVERSAL archive
    // answers for both keys. A release from that era is the one that still
    // carries the Intel command-line tarball; the two went together.
    expect(judgeManifest(manifest('0.6.1'), release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY).ok).toBe(true);
    const universal = manifest('0.6.1', { 'darwin-x86_64': { url: url('0.6.1', TAR), signature: SIG } });
    const universalEra = release('0.6.1', [TAR, `${TAR}.sig`, 'latest.json', 'Screepub-macOS.dmg', INTEL_CLI]);
    expect(judgeManifest(universal, universalEra, { [`${TAR}.sig`]: SIG }, PUBKEY)).toEqual({
      ok: true,
      version: '0.6.1',
      platforms: ['darwin-aarch64', 'darwin-x86_64'],
    });
  });

  test('after Intel was dropped, darwin-x86_64 on the Apple silicon archive is a problem', () => {
    // A release with no Intel command-line tarball has an Apple-silicon-only
    // window archive. Offered to an Intel copy under darwin-x86_64, it would
    // install a build that cannot run there, and the old window would stop
    // opening on that Mac.
    const intelToArm = manifest('0.6.1', { 'darwin-x86_64': { url: url('0.6.1', TAR), signature: SIG } });
    const v = judgeManifest(intelToArm, release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) {
      const text = v.problems.join('\n');
      expect(text).toContain('darwin-x86_64');
      expect(text).toContain(url('0.6.1', TAR));
      expect(text).toMatch(/Apple silicon/);
    }
  });

  test('the same holds for darwin-x86_64-app, which the plugin tries first', () => {
    // tauri-plugin-updater 2.12.0's get_urls looks up `<os>-<arch>-<installer>`
    // before `<os>-<arch>`, so an Intel copy of the .app finds this key first.
    const appKey = manifest('0.6.1', { 'darwin-x86_64-app': { url: url('0.6.1', TAR), signature: SIG } });
    const v = judgeManifest(appKey, release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toContain('darwin-x86_64-app points at');
    // And the arch-specific aarch64 key is an Apple silicon archive too.
    const viaAppArm = JSON.stringify({
      version: '0.6.1',
      platforms: {
        'darwin-aarch64': { url: url('0.6.1', TAR), signature: SIG },
        'darwin-aarch64-app': { url: url('0.6.1', TAR), signature: SIG },
        'darwin-x86_64-app': { url: url('0.6.1', TAR), signature: SIG },
      },
    });
    const w = judgeManifest(viaAppArm, release('0.6.1'), { [`${TAR}.sig`]: SIG }, PUBKEY);
    expect(w.ok).toBe(false);
    // In the universal era both keys may share the archive, -app or not.
    const universalEra = release('0.6.1', [TAR, `${TAR}.sig`, 'latest.json', 'Screepub-macOS.dmg', INTEL_CLI]);
    expect(judgeManifest(appKey, universalEra, { [`${TAR}.sig`]: SIG }, PUBKEY).ok).toBe(true);
  });

  test('every problem is reported, not just the first', () => {
    const v = judgeManifest(
      manifest('0.6.0', { 'darwin-aarch64': { url: url('0.6.0', TAR), signature: SIG } }),
      release('0.6.1', [TAR, 'latest.json']),
      {},
      '',
    );
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.length).toBeGreaterThanOrEqual(3);
  });
});

describe('checking the live endpoint, with the network stubbed', () => {
  /** A fetcher over a url table. Anything not in the table is a 404, which
   *  is what GitHub answers for an asset that does not exist. */
  const table = (answers: Record<string, string>) => {
    const asked: string[] = [];
    const fetcher: Fetcher = async (u) => {
      asked.push(u);
      return u in answers ? { status: 200, text: answers[u]! } : { status: 404, text: 'Not Found' };
    };
    return { asked, fetcher };
  };
  const api = (v: string) =>
    JSON.stringify({
      tag_name: `v${v}`,
      assets: [TAR, `${TAR}.sig`, 'latest.json', 'Screepub-macOS.dmg'].map((name) => ({
        name,
        browser_download_url: url(v, name),
      })),
    });
  const LATEST_API = `https://api.github.com/repos/${REPO}/releases/latest`;
  const ENDPOINT = `https://github.com/${REPO}/releases/latest/download/latest.json`;

  test('it reads the newest release from the API and the manifest from the app’s own url', async () => {
    const { asked, fetcher } = table({
      [LATEST_API]: api('0.6.1'),
      [ENDPOINT]: manifest('0.6.1'),
      [url('0.6.1', `${TAR}.sig`)]: SIG,
    });
    const v = await checkLatest(fetcher, { repo: REPO, pubkey: PUBKEY });
    expect(v).toEqual({ ok: true, version: '0.6.1', platforms: ['darwin-aarch64'] });
    // The manifest comes from the redirecting endpoint, not from a
    // versioned url this tool constructed: what is checked is what the
    // app fetches.
    expect(asked).toContain(ENDPOINT);
    expect(asked).toContain(LATEST_API);
  });

  test('a release with no latest.json at all is the failure this exists for', async () => {
    // Every release before the updater looks like this, and so does one
    // whose app-upload job died. The app fetches, gets a 404, and shows an
    // error; this says so before a user does.
    const { fetcher } = table({ [LATEST_API]: api('0.6.0') });
    const v = await checkLatest(fetcher, { repo: REPO, pubkey: PUBKEY });
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.problems.join('\n')).toMatch(/latest\.json/);
  });

  test('--version asks about that tag instead of the newest release', async () => {
    const TAG_API = `https://api.github.com/repos/${REPO}/releases/tags/v0.6.1`;
    const { asked, fetcher } = table({
      [TAG_API]: api('0.6.1'),
      [ENDPOINT]: manifest('0.6.1'),
      [url('0.6.1', `${TAR}.sig`)]: SIG,
    });
    const v = await checkLatest(fetcher, { repo: REPO, pubkey: PUBKEY, version: 'v0.6.1' });
    expect(v.ok).toBe(true);
    expect(asked).toContain(TAG_API);
    expect(asked).not.toContain(LATEST_API);
  });

  test('an API answer that is not a release is a problem, not a crash', async () => {
    const { fetcher } = table({ [LATEST_API]: '{"message":"Not Found"}' });
    const v = await checkLatest(fetcher, { repo: REPO, pubkey: PUBKEY });
    expect(v.ok).toBe(false);
  });
});
