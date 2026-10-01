// The `settings` verb: a script's own formatting, read and written through
// the sidecar `src/settings/sidecar.ts` already owns. Like the device
// handlers, this RETURNS a result and never prints — cli.ts owns stdout.
import { statSync } from 'node:fs';
import { CliError } from './cli-errors';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions, type FormatOptions } from './options';
import { appDefaultOptions, keepsScriptSettings } from './settings/app-defaults';
import {
  DEVICE_PRESETS,
  matchingPreset,
  type DevicePresetId,
} from './settings/presets';
import { loadScriptSettings, saveScriptSettings, sidecarPath } from './settings/sidecar';

export interface SettingsResult {
  settings: FormatOptions;
  /** Screepub's own shipped defaults (`format-defaults.json`), never the
   * user's app-wide ones: the window uses this to mean "reset to
   * Screepub's own", which has to stay put even when appDefaults differs. */
  defaults: FormatOptions;
  /** The user's app-wide format defaults (piece C): the base this script's
   * settings were read over, when it has no sidecar of its own. */
  appDefaults: FormatOptions;
  preset: DevicePresetId | null;
  presets: { id: DevicePresetId; displayName: string; settings: FormatOptions }[];
  sidecar: string;
  /** The app-wide "When a PDF is converted" choice (`keepsScriptSettings`),
   * carried here beside appDefaults for the same reason: the window's
   * Settings page draws everything it shows from this one answer. */
  keepScriptSettings: boolean;
}

export interface SettingsOptions {
  /** The script's .fountain — the sidecar is named after it. */
  fountain: string;
  /** A PARTIAL FormatOptions as JSON. Overlaid on what is stored, never
   * replacing it, so a window that sends one moved knob moves one knob. */
  set?: string;
  /** Where to read the app-wide format defaults from. Defaults to the
   * production app settings file (`appSettingsPath()`); a test points this
   * at a scratch file so it never touches a real one. */
  appSettingsPath?: string;
}

export function settingsCommand(options: SettingsOptions): SettingsResult {
  let isFile = false;
  try {
    isFile = statSync(options.fountain).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    throw new CliError('unreadable', `cannot read the script: ${options.fountain}`);
  }

  // The app-wide defaults are the base a script with no sidecar of its own
  // reads over, one layer over the shipped defaults (piece C). Read once:
  // both the plain read below and a --set that follows use the SAME base,
  // so a --set on an untouched script overlays onto the user's own tuning
  // rather than resetting it to Screepub's.
  const appDefaults = appDefaultOptions(options.appSettingsPath);
  let settings = loadScriptSettings(options.fountain, appDefaults);

  if (options.set !== undefined) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(options.set);
    } catch {
      throw new CliError('bad-settings', '--set is not valid JSON');
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      throw new CliError('bad-settings', '--set must be a JSON object');
    }
    // Overlaid on the CURRENT settings, not on the defaults: one moved knob
    // must not silently reset the other nineteen. Clamping is
    // resolveFormatOptions', not a second copy of it.
    settings = resolveFormatOptions(parsed as Record<string, unknown>, settings);
    saveScriptSettings(settings, options.fountain);
  }

  const ids = Object.keys(DEVICE_PRESETS) as DevicePresetId[];
  return {
    settings,
    defaults: DEFAULT_FORMAT_OPTIONS,
    appDefaults,
    preset: matchingPreset(settings),
    presets: ids.map((id) => ({
      id,
      displayName: DEVICE_PRESETS[id].displayName,
      settings: DEVICE_PRESETS[id].settings,
    })),
    sidecar: sidecarPath(options.fountain),
    keepScriptSettings: keepsScriptSettings(options.appSettingsPath),
  };
}
