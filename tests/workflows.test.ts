// Every workflow in .github/workflows, read as the YAML GitHub will run.
//
// Two things that only GitHub would otherwise find out, and only after a
// push: a `needs:` naming a job that is no longer there (GitHub rejects the
// whole workflow file, so every job in it stops running, silently, until
// someone opens the Actions tab), and a step that still builds or runs the
// Swift Mac app after piece F3 deleted it (F3 acceptance criterion 15: no
// workflow references a Swift job).
import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

type Step = { run?: string; uses?: string; 'working-directory'?: string };
type Job = { needs?: string | string[]; steps?: Step[]; defaults?: { run?: { 'working-directory'?: string } } };
type Workflow = { jobs: Record<string, Job> };

const DIR = join(new URL('..', import.meta.url).pathname, '.github', 'workflows');
const workflows: [string, Workflow][] = readdirSync(DIR)
  .filter((f) => f.endsWith('.yml') || f.endsWith('.yaml'))
  .sort()
  .map((f) => [f, Bun.YAML.parse(readFileSync(join(DIR, f), 'utf8')) as Workflow]);

const needsOf = (job: Job): string[] => (job.needs === undefined ? [] : [job.needs].flat());

describe('the workflows', () => {
  test('are all found and parsed', () => {
    // A wrong directory or a parse that yields nothing would make every
    // test below pass over an empty list.
    const names = workflows.map(([f]) => f);
    for (const f of ['ci.yml', 'desktop.yml', 'release.yml', 'tap-freshness.yml', 'weekly-toolchain.yml']) {
      expect(names).toContain(f);
    }
    for (const [f, wf] of workflows) expect(Object.keys(wf.jobs ?? {}).length, f).toBeGreaterThan(0);
  });

  test('every needs: names a job that exists in the same workflow', () => {
    const dangling: string[] = [];
    for (const [f, wf] of workflows) {
      for (const [name, job] of Object.entries(wf.jobs)) {
        for (const n of needsOf(job)) if (!(n in wf.jobs)) dangling.push(`${f}: ${name} needs ${n}`);
      }
    }
    expect(dangling).toEqual([]);
  });

  test('no job builds or runs the Swift app, which is gone', () => {
    const swift: string[] = [];
    for (const [f, wf] of workflows) {
      for (const [name, job] of Object.entries(wf.jobs)) {
        const dirs = [job.defaults?.run?.['working-directory'], ...(job.steps ?? []).map((s) => s['working-directory'])];
        for (const d of dirs) if (d !== undefined && /^app(\/|$)/.test(d)) swift.push(`${f}: ${name} works in ${d}`);
        for (const step of job.steps ?? []) {
          if (/\bswift\s+(build|run|test|package)\b/.test(step.run ?? '')) swift.push(`${f}: ${name} runs ${step.run!.trim()}`);
        }
      }
    }
    expect(swift).toEqual([]);
  });
});
