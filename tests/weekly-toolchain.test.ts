// .github/workflows/weekly-toolchain.yml's engine-calibre job can only be
// run by GitHub, weekly. These are the parts of it that can be checked
// here, so a typo in a test path or a stray working-directory is caught on
// the next push instead of on a Monday.
import { describe, expect, test } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';

type Step = { uses?: string; run?: string; name?: string; 'working-directory'?: string; with?: Record<string, unknown> };
type Workflow = { jobs: Record<string, { 'runs-on': string; steps: Step[] }> };

const YML = '.github/workflows/weekly-toolchain.yml';
const weekly = Bun.YAML.parse(readFileSync(YML, 'utf8')) as Workflow;
const ci = Bun.YAML.parse(readFileSync('.github/workflows/ci.yml', 'utf8')) as Workflow;
const job = weekly.jobs['engine-calibre'];

describe('weekly-toolchain.yml: the engine Calibre job', () => {
  test('exists, on macOS', () => {
    expect(job, 'no engine-calibre job').toBeDefined();
    expect(job!['runs-on']).toMatch(/^macos-/);
  });

  // It survived the Swift job and its folder being deleted together (F3),
  // and must stay independent of anything outside the repo root.
  test('runs from the repo root with bun only, nothing from the Swift app', () => {
    for (const step of job!.steps) {
      expect(step['working-directory'], JSON.stringify(step)).toBeUndefined();
      expect(step.run ?? '').not.toMatch(/swift|(?<![\w./-])app\//);
    }
  });

  test('installs Calibre, then checks the engine finds it before testing', () => {
    const runs = job!.steps.map((s) => s.run ?? '');
    const install = runs.findIndex((r) => r.includes('brew install --cask calibre'));
    const finds = runs.findIndex((r) => r.includes("calibreTool(t)"));
    const tests = runs.findIndex((r) => r.startsWith('bun test '));
    expect(install).toBeGreaterThan(-1);
    expect(finds).toBeGreaterThan(install);
    expect(tests).toBeGreaterThan(finds);
  });

  test('every test file it names exists', () => {
    const cmd = job!.steps.map((s) => s.run ?? '').find((r) => r.startsWith('bun test '))!;
    const files = cmd.split(/\s+/).filter((w) => w.endsWith('.test.ts'));
    expect(files).toContain('tests/export-calibre.test.ts');
    for (const f of files) expect(existsSync(f), f).toBe(true);
  });

  // Same pins as ci.yml, so a pin bump there that misses this file shows.
  test('pins every action by full SHA, the same SHAs and bun version ci.yml uses', () => {
    const ciPins = new Set(Object.values(ci.jobs).flatMap((j) => j.steps.map((s) => s.uses).filter(Boolean)));
    for (const step of job!.steps.filter((s) => s.uses)) {
      expect(step.uses).toMatch(/@[0-9a-f]{40}$/);
      expect(ciPins.has(step.uses), `${step.uses} differs from ci.yml`).toBe(true);
    }
    const bun = job!.steps.find((s) => s.uses?.startsWith('oven-sh/setup-bun@'));
    const ciBun = ci.jobs.engine!.steps.find((s) => s.uses?.startsWith('oven-sh/setup-bun@'));
    expect(bun?.with?.['bun-version']).toBe(ciBun?.with?.['bun-version']);
  });
});
