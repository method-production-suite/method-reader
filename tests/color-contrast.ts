/// Colour arithmetic for tests/brand-tokens.test.ts's contrast table: hex
/// in, WCAG 2.1 contrast ratio out.
///
/// This file was tests/theme-colors.ts until piece F3. Then it also parsed
/// the colour literals out of the Swift Mac app's theme, because the
/// brand tokens were first pinned FROM that theme and, after 2026-09-23,
/// the theme was checked against them. The theme left with the Swift app,
/// so that parser and the comparison went with it (see them at any tag
/// before F3, e.g. v0.7.4). brand/tokens.json is the only source of the
/// colours now.
export type Rgba = { r: number; g: number; b: number; a: number };

export function fromHex(h: string): Rgba {
  const s = h.replace('#', '');
  const w = s.length === 3 ? s.split('').map((c) => c + c).join('') : s;
  return {
    r: parseInt(w.slice(0, 2), 16),
    g: parseInt(w.slice(2, 4), 16),
    b: parseInt(w.slice(4, 6), 16),
    a: 1,
  };
}

/// WCAG 2.1 relative luminance and contrast ratio.
export function luminance({ r, g, b }: Rgba): number {
  const f = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}
