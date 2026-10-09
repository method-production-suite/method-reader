// What to tell a person about the KFX rung, and what they can do about it.
//
// kfx.ts PROBES (is Calibre there, is Previewer there, is the plugin in
// Calibre) and formats.ts owns the LADDER (KFX, else AZW3, else MOBI). This
// file turns the probe into a three-step checklist with one fix per missing
// step and one sentence saying what a Kindle gets today. The CLI's
// `kfx-status` prints it and the window's send panel draws it, so the two
// tell the same story from one place. Pure: no probe, no spawn, no network.
import { fileExtension } from './formats';
import type { KfxStatus } from './kfx';

/** How to fix one missing step. */
export type KfxFix =
  /** Only the user can install it; this page is where. */
  | { kind: 'link'; label: string; url: string }
  /** Screepub can install it: `screepub kfx-install`. */
  | { kind: 'install'; label: string }
  /** Needs another step done first. */
  | { kind: 'after'; why: string }
  /** Cannot be had on this platform at all. */
  | { kind: 'unavailable'; why: string };

export interface KfxStep {
  id: 'calibre' | 'previewer' | 'plugin';
  name: string;
  installed: boolean;
  /** Null exactly when installed. */
  fix: KfxFix | null;
}

export interface KfxSetup {
  /** KfxStatus.ready, passed through. */
  ready: boolean;
  /** False where Amazon makes no Kindle Previewer, so KFX cannot happen. */
  possible: boolean;
  /** One sentence: what a Kindle gets today. */
  summary: string;
  /** Always three, in this order: calibre, previewer, plugin. */
  steps: KfxStep[];
}

const CALIBRE_PAGES: Record<string, string> = {
  darwin: 'https://calibre-ebook.com/download_osx',
  win32: 'https://calibre-ebook.com/download_windows',
};
/** Calibre's chooser page, for a platform with no page of its own. Only
 *  emitted where `possible` is false, which the window never draws, so it is
 *  deliberately NOT in KFX_LINKS and not in the window's opener grant. */
const CALIBRE_ANY = 'https://calibre-ebook.com/download';
/** Amazon's Kindle Previewer page. One page carries both the Mac and the
 *  Windows download (checked 2026-09-23). */
const PREVIEWER_PAGE = 'https://kdp.amazon.com/en_US/help/topic/G202131170';

/** Every URL a link fix can carry where KFX is possible. The window's opener
 *  grant allows exactly these, and tests/desktop-shell.test.ts holds the two
 *  together, so the engine cannot draw a link the window may not open. */
export const KFX_LINKS: readonly string[] = [...Object.values(CALIBRE_PAGES), PREVIEWER_PAGE];

/** Amazon makes Kindle Previewer for macOS and Windows and nothing else;
 *  kfx.ts's previewerPath() has a branch for exactly these two. */
export function kfxPossible(platform: string): boolean {
  return platform === 'darwin' || platform === 'win32';
}

/** "Linux" on Linux, "this system" everywhere else Amazon makes no Kindle
 *  Previewer for. Exported so cli-kfx.ts's install-time refusal names the
 *  same system kfxSetup's own checklist would, from one place rather than a
 *  second copy of the choice. */
export function systemName(platform: string): string {
  return platform === 'linux' ? 'Linux' : 'this system';
}

function calibreFix(status: KfxStatus, platform: string): KfxFix | null {
  if (status.calibre) return null;
  return { kind: 'link', label: 'Get Calibre', url: CALIBRE_PAGES[platform] ?? CALIBRE_ANY };
}

function previewerFix(status: KfxStatus, platform: string): KfxFix | null {
  if (status.previewer) return null;
  if (!kfxPossible(platform)) {
    return { kind: 'unavailable', why: `Amazon does not make it for ${systemName(platform)}` };
  }
  return { kind: 'link', label: 'Get Kindle Previewer', url: PREVIEWER_PAGE };
}

function pluginFix(status: KfxStatus, installed: boolean, possible: boolean): KfxFix | null {
  if (installed) return null;
  // Off darwin/win32 there is no Kindle Previewer for the plugin to drive,
  // so it is useless there: neither "install it" nor "install Calibre
  // first" leads anywhere a Linux user can act on.
  if (!possible) return { kind: 'unavailable', why: 'Of no use without Kindle Previewer' };
  if (!status.calibre) return { kind: 'after', why: 'Install Calibre first' };
  return { kind: 'install', label: 'Install' };
}

export function kfxSetup(status: KfxStatus, platform: string): KfxSetup {
  const possible = kfxPossible(platform);
  // KfxStatus documents pluginInstalled as meaningful only with Calibre:
  // the plugin lives inside it.
  const pluginInstalled = status.calibre && status.pluginInstalled;
  const steps: KfxStep[] = [
    { id: 'calibre', name: 'Calibre', installed: status.calibre, fix: calibreFix(status, platform) },
    {
      id: 'previewer',
      name: 'Kindle Previewer',
      installed: status.previewer,
      fix: previewerFix(status, platform),
    },
    {
      id: 'plugin',
      name: 'KFX plugin',
      installed: pluginInstalled,
      fix: pluginFix(status, pluginInstalled, possible),
    },
  ];
  // The ladder's own answer for "not KFX", so this sentence cannot disagree
  // with what the export actually builds.
  const today = fileExtension('kindle', { calibreAvailable: status.calibre, kfxReady: false }).toUpperCase();
  let summary: string;
  if (status.ready) {
    summary = 'Kindles get KFX, the best quality Screepub can make.';
  } else if (!possible) {
    summary = `Amazon does not make Kindle Previewer for ${systemName(platform)}, so Kindles get ${today}.`;
  } else {
    summary = `Kindles get ${today} for now. KFX looks better, and needs the three free tools below.`;
  }
  return { ready: status.ready, possible, summary, steps };
}
