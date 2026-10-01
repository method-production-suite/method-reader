// Does the PUBLISHED latest.json describe the newest release?
//
//   bun tools/check-latest.ts                 # the newest non-prerelease release
//   bun tools/check-latest.ts --version v0.6.1
//
// Exit 0 when the manifest the app will fetch names the newest release,
// points every platform at an asset that release carries, pastes the
// exact contents of each asset's .sig, and is signed by the key the app
// is configured to trust. Exit 1 otherwise, having said which part is
// wrong. Every problem is reported, not only the first.
//
// This is the updater's version of tools/check-tap.sh, for the same
// reason. The Homebrew tap served 0.3.0 for five releases because nothing
// compared the published file to the newest release. A stale or missing
// latest.json fails quieter still: the app fetches, gets a 404 or an old
// version, shows an error or nothing, and the only person who ever finds
// out is a user.
//
// So the manifest is read from the EXACT url the app reads it from,
// `releases/latest/download/latest.json`, with GitHub's redirect, never
// from a checkout, a build directory or a versioned url this tool
// constructed. What is checked is what the app fetches.
//
// Runs in release.yml's latest-check job (per release) and in
// tap-freshness.yml (weekly). Needs no secret; the repository is public.
// GH_TOKEN is honoured for the API call when present, for rate limits.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { REPO_DIR } from './build-cli';
import { MAC_ARCH, UPDATER_KINDS, type PlatformKey } from './build-app-bundle';
import { MANIFEST_NAME, RELEASE_REPO, manifestEndpoint } from './build-update-manifest';
import { parsePublicKeyBox, parseSignatureBox } from './update-signature';

export interface ReleaseAsset {
  name: string;
  /** The `browser_download_url`: the exact string the manifest must carry. */
  url: string;
}

export interface Release {
  tagName: string;
  assets: ReleaseAsset[];
}

export interface Fetched {
  status: number;
  text: string;
}

/** The network, injected. Follows redirects, because the endpoint is one. */
export type Fetcher = (url: string) => Promise<Fetched>;

export type Verdict =
  | { ok: true; version: string; platforms: string[] }
  | { ok: false; problems: string[] };

/** The Intel command-line tarball. The identifier release is the last to
 *  carry it, and the last whose window archive is universal: the two were
 *  dropped together (plan 2026-10-01-f2-handover-amendment.md). The asset
 *  list names nothing else that tells the two eras apart, since the window
 *  archive keeps its name. */
export const INTEL_CLI_ASSET = 'screepub-cli-macos-x64.tar.gz';

/** Every platform the macOS build must answer for: darwin-aarch64 alone
 *  since the release after the identifier release (Apple silicon only).
 *  Derived from the same table that names the artifacts, so a new row
 *  there widens this automatically.
 *
 *  REQUIRED, not exhaustive: this runs weekly against whatever release is
 *  newest, and until the Apple-silicon-only release ships that is the
 *  identifier release, whose universal archive answers for both keys. A
 *  darwin-x86_64 row is judged per release in judgeManifest. */
export function requiredPlatforms(): PlatformKey[] {
  return UPDATER_KINDS.filter((k) => k.os === 'macos').flatMap((k) => k.platformKeys(MAC_ARCH));
}

interface ManifestShape {
  version?: unknown;
  platforms?: Record<string, { url?: unknown; signature?: unknown }>;
}

/** The judgement, pure. `sigTexts` is the contents of each `.sig` asset
 *  the release carries, keyed by asset name; `pubkey` is what the app is
 *  configured to trust (tauri.conf.json, plugins.updater.pubkey). */
export function judgeManifest(
  manifestText: string,
  release: Release,
  sigTexts: Record<string, string>,
  pubkey: string,
): Verdict {
  const problems: string[] = [];
  const version = release.tagName.replace(/^v/, '');

  let manifest: ManifestShape;
  try {
    manifest = JSON.parse(manifestText) as ManifestShape;
  } catch {
    return { ok: false, problems: [`${MANIFEST_NAME} is not JSON (begins ${JSON.stringify(manifestText.slice(0, 60))})`] };
  }
  if (typeof manifest !== 'object' || manifest === null) {
    return { ok: false, problems: [`${MANIFEST_NAME} is not a JSON object`] };
  }

  if (manifest.version !== version) {
    problems.push(
      `${MANIFEST_NAME} says version ${JSON.stringify(manifest.version)} but the newest release is ${version}`,
    );
  }

  const platforms =
    typeof manifest.platforms === 'object' && manifest.platforms !== null ? manifest.platforms : {};
  const keys = Object.keys(platforms).sort();
  if (keys.length === 0) {
    problems.push(`${MANIFEST_NAME} lists no platforms at all`);
  }
  for (const required of requiredPlatforms()) {
    if (!keys.includes(required)) {
      problems.push(`${MANIFEST_NAME} has no entry for ${required}, which the macOS build must serve`);
    }
  }

  let trusted: string | undefined;
  try {
    trusted = parsePublicKeyBox(pubkey).keyId;
  } catch (err) {
    problems.push(`the app's configured public key cannot be read: ${(err as Error).message}`);
  }

  // After Intel was dropped the one window archive is Apple silicon only.
  // A darwin-x86_64 row pointing at it would hand an Intel copy a build it
  // cannot run. Before (a release still carrying the Intel tarball), the
  // archive is universal and the same url answers both keys, rightly.
  const intelEra = release.assets.some((a) => a.name === INTEL_CLI_ASSET);
  const x86 = platforms['darwin-x86_64'];
  const arm = platforms['darwin-aarch64'];
  if (!intelEra && x86 && arm && typeof x86.url === 'string' && x86.url === arm.url) {
    problems.push(
      `darwin-x86_64 points at ${x86.url}, the same archive as darwin-aarch64, and release ` +
        `${release.tagName} is Apple silicon only (it carries no ${INTEL_CLI_ASSET}): an Intel copy ` +
        'would install a build it cannot run',
    );
  }

  const assetsByUrl = new Map(release.assets.map((a) => [a.url, a]));
  for (const key of keys) {
    const entry = platforms[key]!;
    const url = typeof entry.url === 'string' ? entry.url : '';
    const signature = typeof entry.signature === 'string' ? entry.signature : '';
    const asset = assetsByUrl.get(url);
    if (!asset) {
      problems.push(`${key} points at ${url || '<no url>'}, which is not an asset of release ${release.tagName}`);
      continue;
    }
    const sigName = `${asset.name}.sig`;
    const published = sigTexts[sigName];
    if (published === undefined) {
      problems.push(`release ${release.tagName} carries no ${sigName} beside ${asset.name}`);
    } else if (published !== signature) {
      problems.push(`${key}: the signature in ${MANIFEST_NAME} is not the contents of ${sigName}`);
    }
    try {
      const { keyId } = parseSignatureBox(signature);
      if (trusted !== undefined && keyId !== trusted) {
        problems.push(
          `${key}: signed by key ${keyId}, but the app trusts key ${trusted}. tauri-cli only warns ` +
            'about this; every install would fail.',
        );
      }
    } catch (err) {
      problems.push(`${key}: the signature is not a minisign signature: ${(err as Error).message}`);
    }
  }

  return problems.length === 0 ? { ok: true, version, platforms: keys } : { ok: false, problems };
}

interface GitHubRelease {
  tag_name?: string;
  assets?: { name: string; browser_download_url: string }[];
}

/** Fetch the release, the manifest and every signature, then judge. */
export async function checkLatest(
  fetcher: Fetcher,
  opts: { repo: string; pubkey: string; version?: string },
): Promise<Verdict> {
  const apiUrl = opts.version
    ? `https://api.github.com/repos/${opts.repo}/releases/tags/v${opts.version.replace(/^v/, '')}`
    : `https://api.github.com/repos/${opts.repo}/releases/latest`;
  const api = await fetcher(apiUrl);
  let parsed: GitHubRelease;
  try {
    parsed = JSON.parse(api.text) as GitHubRelease;
  } catch {
    return { ok: false, problems: [`${apiUrl} did not answer JSON (HTTP ${api.status})`] };
  }
  if (api.status !== 200 || typeof parsed.tag_name !== 'string' || !Array.isArray(parsed.assets)) {
    return {
      ok: false,
      problems: [`${apiUrl} did not describe a release (HTTP ${api.status}: ${api.text.slice(0, 120)})`],
    };
  }
  const release: Release = {
    tagName: parsed.tag_name,
    assets: parsed.assets.map((a) => ({ name: a.name, url: a.browser_download_url })),
  };

  const endpoint = manifestEndpoint(opts.repo);
  const manifest = await fetcher(endpoint);
  if (manifest.status !== 200) {
    return {
      ok: false,
      problems: [
        `${endpoint} answered HTTP ${manifest.status}: release ${release.tagName} publishes no ` +
          `${MANIFEST_NAME}, so the app's update check fails on every machine that makes it.`,
      ],
    };
  }

  const sigTexts: Record<string, string> = {};
  for (const asset of release.assets) {
    if (!asset.name.endsWith('.sig')) continue;
    const got = await fetcher(asset.url);
    if (got.status === 200) sigTexts[asset.name] = got.text;
  }

  return judgeManifest(manifest.text, release, sigTexts, opts.pubkey);
}

/** What the shipped app trusts, from the checkout's tauri.conf.json. */
export function configuredPubkey(repoDir: string = REPO_DIR): string {
  const conf = JSON.parse(
    readFileSync(join(repoDir, 'desktop', 'src-tauri', 'tauri.conf.json'), 'utf8'),
  ) as { plugins?: { updater?: { pubkey?: string } } };
  return conf.plugins?.updater?.pubkey ?? '';
}

const realFetcher: Fetcher = async (url) => {
  const headers: Record<string, string> = { 'User-Agent': 'screepub-check-latest' };
  if (url.startsWith('https://api.github.com/')) {
    headers.Accept = 'application/vnd.github+json';
    if (process.env.GH_TOKEN) headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;
  }
  const res = await fetch(url, { headers, redirect: 'follow' });
  return { status: res.status, text: await res.text() };
};

if (import.meta.main) {
  const { values } = parseArgs({
    args: Bun.argv.slice(2),
    options: {
      version: { type: 'string' },
      repo: { type: 'string', default: RELEASE_REPO },
    },
    strict: true,
    allowPositionals: false,
  });
  const verdict = await checkLatest(realFetcher, {
    repo: values.repo ?? RELEASE_REPO,
    pubkey: configuredPubkey(),
    version: values.version,
  });
  if (verdict.ok) {
    console.log(`${MANIFEST_NAME} serves ${verdict.version} for ${verdict.platforms.join(', ')}, signed by the key the app trusts.`);
  } else {
    for (const p of verdict.problems) console.error(`::error::${p}`);
    console.error(
      `::notice::A red latest-check means the release published and the app's updater cannot use it. ` +
        'app-upload writes latest.json from what arrived; see docs/release-secrets.md §4.',
    );
    process.exit(1);
  }
}
