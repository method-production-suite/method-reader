// Named bundles of FormatOptions tuned for a class of reading device.
// Responsive reflow is impossible in a fixed e-book (no JS, media queries
// stripped), so a preset chosen at conversion time is the mechanism for
// device-appropriate geometry.
import { DEFAULT_FORMAT_OPTIONS, type FormatOptions } from '../options';

export type DevicePresetId = 'kindleEink' | 'phone';

export const DEVICE_PRESETS: Record<DevicePresetId, { displayName: string; settings: FormatOptions }> = {
  /** The recommended baseline: 6" e-ink Kindle. Identical to defaults — but
   * a COPY of them. Swift's FormatOptions was a value type, so aliasing the
   * defaults was impossible there; here a preset handed out by reference IS
   * DEFAULT_FORMAT_OPTIONS, and one stray mutation through it would corrupt
   * every conversion in the process — and desync the defaults that
   * options.test.ts pins to format-defaults.json. */
  kindleEink: {
    displayName: 'Kindle e-ink (6")',
    settings: { ...DEFAULT_FORMAT_OPTIONS },
  },
  /** Narrow phone/tablet reading app: side-by-side dual dialogue is an
   * unreadable sliver, so speeches go sequential, and the dialogue column
   * widens (shallower side margins) to use the small screen. */
  phone: {
    displayName: 'Phone / narrow screen',
    settings: { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 10, dualDialogue: 'sequential' },
  },
};

/** The preset whose settings exactly match `settings`, or null when they have
 * been tuned away from every preset. Applying a preset overwrites the
 * settings and stores no identity, so equality is the only honest answer to
 * "which preset am I on?" — a remembered name would keep claiming "Kindle
 * e-ink" after the first knob moved. */
export function matchingPreset(settings: FormatOptions): DevicePresetId | null {
  const ids = Object.keys(DEVICE_PRESETS) as DevicePresetId[];
  return ids.find((id) => sameSettings(DEVICE_PRESETS[id].settings, settings)) ?? null;
}

function sameSettings(a: FormatOptions, b: FormatOptions): boolean {
  const keys = Object.keys(a) as Array<keyof FormatOptions>;
  return keys.every((k) => a[k] === b[k]);
}
