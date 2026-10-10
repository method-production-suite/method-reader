// What "beta" marks, said the same way in every place a person meets it:
// the CLI's help, the README and the site. The window's badge placement is
// pinned where each surface is mounted (send-panel-ui.test.ts for the send
// panel, desktop-ui.test.ts for Tune); the badge itself and the one pure
// decision behind it are pinned here.
//
// Beta, here: it works in our tests, but has not been tried widely on real
// devices or apps yet. Five things carry it: FDX export, pictures in books
// (the title card as the cover among them), eject on Windows and Linux, the
// device-request and route-report buttons, and the AZW3 Screepub writes
// itself.
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dir, '..');
const UI = join(ROOT, 'desktop', 'ui');
const help = (...args: string[]) => {
  const proc = Bun.spawnSync(['bun', 'src/cli.ts', ...args], { cwd: ROOT, env: process.env });
  return proc.stdout.toString();
};
const flat = (s: string) => s.replace(/\s+/g, ' ');
/** The help line that starts with `start`, with its wrapped continuation. */
const helpEntry = (text: string, start: string) => {
  const lines = text.split('\n');
  const i = lines.findIndex((l) => l.trimStart().startsWith(start));
  expect(i).toBeGreaterThanOrEqual(0);
  const out = [lines[i]];
  for (let j = i + 1; j < lines.length && /^ {20,}\S/.test(lines[j]); j++) out.push(lines[j]);
  return flat(out.join(' '));
};

describe('the CLI help says beta beside each beta feature', () => {
  test('--fdx on a conversion', () => {
    expect(helpEntry(help('--help'), '--fdx <file>')).toContain('(beta)');
  });

  test('--azw3 on a conversion, and the AZW3 in export', () => {
    expect(helpEntry(help('--help'), '--azw3 <file>')).toContain('(beta)');
    const text = help('export', '--help');
    expect(flat(text)).toContain('an AZW3 (beta) that Screepub writes itself');
    expect(helpEntry(text, '--for <type>')).toContain('azw3 (beta)');
  });

  test('pictures in books, in the conversion help', () => {
    const text = flat(help('--help'));
    expect(text).toContain('Pictures (beta) carried out of a PDF');
    // And what the .mobi does about them, as long as the gate is shut.
    expect(text).toContain('The .mobi is text-only for now');
  });

  test('export --for fdx', () => {
    const text = help('export', '--help');
    expect(helpEntry(text, 'screepub export <file.epub> --for fdx')).toContain('(beta)');
    expect(flat(text)).toContain('fdx (beta) is the script itself');
  });

  test('eject, on Windows and Linux only', () => {
    expect(helpEntry(help('--help'), 'screepub eject <volume>')).toContain('(beta on Windows and Linux)');
    expect(flat(help('eject', '--help'))).toContain('beta on Windows and Linux');
  });

  test('beta is said exactly where it belongs, and nowhere else', () => {
    const count = (t: string) => (t.match(/beta/gi) ?? []).length;
    // Pictures, --fdx, --azw3 and eject in the conversion help.
    expect(count(help('--help'))).toBe(4);
    // The fdx usage line, its paragraph, the AZW3 in the kindle paragraph,
    // and azw3 and fdx in the --for list.
    expect(count(help('export', '--help'))).toBe(5);
    expect(count(help('eject', '--help'))).toBe(1);
  });
});

describe('the badge', () => {
  test('is real text with a description, built in one place', () => {
    const src = readFileSync(join(UI, 'beta.js'), 'utf8');
    expect(src).toContain("'aria-description'");
    expect(src).toContain('title');
    for (const f of ['send-panel.js', 'tune.js']) {
      const code = readFileSync(join(UI, f), 'utf8');
      expect(code).toContain("from './beta.js'");
      expect(code).not.toContain("'beta-badge'");
    }
  });

  test('is styled with the design tokens only', () => {
    const css = readFileSync(join(UI, 'surfaces.css'), 'utf8');
    // The badge's own rule, at the start of a line (a card's placement
    // rule, `.device-card-slot > .beta-badge`, only positions it).
    const start = css.indexOf('\n.beta-badge {');
    expect(start).toBeGreaterThan(0);
    const rule = css.slice(start, css.indexOf('}', start));
    expect(rule).not.toMatch(/#[0-9a-f]{3,8}\b|rgb|hsl/i);
    const colours = [...rule.matchAll(/(?:^|[\s;{])(?:color|border-color|background):\s*([^;]+);/g)];
    expect(colours.length).toBeGreaterThan(0);
    for (const m of colours) expect(m[1]).toMatch(/var\(--/);
  });

  test('eject is beta everywhere but a Mac', async () => {
    const D = await import(join(UI, 'device.js'));
    expect(D.ejectIsBeta('MacIntel')).toBe(false);
    expect(D.ejectIsBeta('macOS')).toBe(false);
    expect(D.ejectIsBeta('Win32')).toBe(true);
    expect(D.ejectIsBeta('Windows')).toBe(true);
    expect(D.ejectIsBeta('Linux x86_64')).toBe(true);
  });
});

describe('README and site say what beta means, and mark the four', () => {
  const MEANING = /works in our tests, but has not been tried widely on real devices or apps yet/;
  for (const [name, file] of [['README', 'README.md'], ['site', 'site/index.html']] as const) {
    test(name, () => {
      const text = flat(readFileSync(join(ROOT, file), 'utf8'));
      expect(text).toMatch(MEANING);
      expect(text).not.toContain('—');
      expect(text).toMatch(/FDX[^.]{0,120}\(beta\)/);
      expect(text).toMatch(/[Pp]ictures[^.]{0,160}\(beta\)/);
      expect(text).toMatch(/[Ee]ject[^.]{0,160}\(beta on Windows and Linux\)/);
      expect(text).toMatch(/isn.t here[^.]{0,200}\(beta\)/);
    });
  }
});
