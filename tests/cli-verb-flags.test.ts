// Every verb refuses every flag that is not its own.
//
// The verbs share ONE parser (src/cli.ts parseVerbArgs), so every flag any
// verb takes parses for all of them, and each verb has to refuse the ones it
// cannot act on itself. That was done verb by verb, and two were missed:
// `settings` quietly accepted --device, --for, --fountain and --options-json,
// and `export` accepted --device and --set. A flag that silently does
// nothing teaches the caller it did something, which is the rule every
// other refusal in cli.ts is written to.
//
// So this checks the whole matrix, not the two known cases: every verb in
// VERBS against every flag in the shared parser. A new verb, or a new flag
// on an old one, fails here until it says which flags it owns. Every
// positional is a path that does not exist, so a refusal that answered
// only AFTER reading the file would come back as `unreadable`, not `usage`:
// this also pins that the refusal comes first.
import { afterAll, describe, expect, test } from 'bun:test';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { VERBS, type Verb } from '../src/cli-devices';

const ROOT = new URL('..', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-verb-flags-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

async function runCli(args: string[]) {
  const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], { stdout: 'pipe', stderr: 'pipe' });
  const [stdout, exitCode] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  return { stdout, exitCode };
}

/** Every flag in parseVerbArgs' shared schema but --json and --help, with a
 *  value where it takes one. */
const FLAGS: Record<string, string[]> = {
  '--device': ['--device', 'kindle-1'],
  '--set': ['--set', '{}'],
  '--for': ['--for', 'kindle'],
  '--fountain': ['--fountain', '/nowhere/x.fountain'],
  '--options-json': ['--options-json', '{}'],
  '--out': ['--out', '/nowhere/out.epub'],
  '--offered': ['--offered', '1.0.0'],
  '--current': ['--current', '1.0.0'],
  '--opted-in': ['--opted-in'],
  '--last-checked': ['--last-checked', '5'],
  '--quick': ['--quick'],
  '--check': ['--check'],
};

/** What each verb owns, and a well-formed set of positionals for it. */
const VERB_SHAPE: Record<Verb, { own: string[]; positionals: string[] }> = {
  devices: { own: ['--quick'], positionals: [] },
  send: { own: ['--device'], positionals: ['/nowhere/x.epub'] },
  settings: { own: ['--set'], positionals: ['/nowhere/x.fountain'] },
  export: { own: ['--for', '--fountain', '--options-json', '--out', '--check'], positionals: ['/nowhere/x.epub'] },
  'update-decision': { own: ['--offered', '--current'], positionals: [] },
  'update-should-check': { own: ['--opted-in', '--last-checked'], positionals: [] },
  'kfx-status': { own: [], positionals: [] },
  'kfx-install': { own: [], positionals: [] },
  'app-settings': { own: ['--set'], positionals: [] },
  reveal: { own: [], positionals: ['/nowhere/x.epub'] },
  routes: { own: ['--quick'], positionals: ['/nowhere/x.epub'] },
  route: { own: ['--out', '--fountain', '--options-json'], positionals: ['save-kindle', '/nowhere/x.epub'] },
  eject: { own: [], positionals: ['/nowhere/Kindle'] },
};

describe('every verb refuses every flag that is not its own', () => {
  test('the table covers every verb and every shared flag, so nothing new slips past it', async () => {
    expect(Object.keys(VERB_SHAPE).sort()).toEqual([...VERBS].sort());
    // Read the shared schema out of cli.ts itself: a flag added there that
    // this table does not know fails here rather than going unchecked.
    const source = await Bun.file(`${ROOT}src/cli.ts`).text();
    const schema = source.slice(source.indexOf('function parseVerbArgs('), source.indexOf('async function runVerb('));
    const declared = [...schema.matchAll(/^\s+'?([a-z][a-z-]*)'?: \{ type:/gm)]
      .map((m) => `--${m[1]}`)
      .filter((flag) => flag !== '--json' && flag !== '--help');
    expect(declared.sort()).toEqual(Object.keys(FLAGS).sort());
  });

  /** The whole refusal, owners and all. The owners come from VERB_SHAPE in
   *  VERBS order, so a refusal that names the wrong owner, or leaves one
   *  out, fails here and not only in a per-verb file. This is what let the
   *  per-verb refusal tables (cli-app-settings, cli-reveal, cli-routes,
   *  cli-kfx, cli-export, cli-device-commands) fold into this one. */
  function refusal(verb: Verb, flag: string): string {
    const owners = VERBS.filter((v) => VERB_SHAPE[v].own.includes(flag)).join(' and ');
    const aside = verb === 'devices' && flag === '--device' ? ': it lists every reader' : '';
    return `${verb} takes no ${flag}${aside} (${flag} belongs to ${owners})`;
  }

  for (const verb of VERBS) {
    const { own, positionals } = VERB_SHAPE[verb];
    const foreign = Object.keys(FLAGS).filter((flag) => !own.includes(flag));
    test(`${verb} refuses ${foreign.join(', ')}`, async () => {
      const answers = await Promise.all(foreign.map(async (flag) => {
        const { stdout, exitCode } = await runCli([verb, ...positionals, ...FLAGS[flag], '--json']);
        const answer = JSON.parse(stdout);
        return `${flag}: ${exitCode} ${answer.error?.code} ${answer.error?.message}`;
      }));
      expect(answers).toEqual(foreign.map((flag) => `${flag}: 1 usage ${refusal(verb, flag)}`));
    }, 60000);
  }
});

describe('the two verbs that used to let foreign flags through', () => {
  // Pinned whole, in the wording every other verb already uses.
  const CASES: [string[], string][] = [
    [['settings', '/nowhere/x.fountain', '--device', 'kindle-1'],
      'settings takes no --device (--device belongs to send)'],
    [['settings', '/nowhere/x.fountain', '--for', 'kindle'],
      'settings takes no --for (--for belongs to export)'],
    [['settings', '/nowhere/x.fountain', '--fountain', '/nowhere/y.fountain'],
      'settings takes no --fountain (--fountain belongs to export and route)'],
    [['settings', '/nowhere/x.fountain', '--options-json', '{}'],
      'settings takes no --options-json (--options-json belongs to export and route)'],
    [['export', '/nowhere/x.epub', '--device', 'kindle-1'],
      'export takes no --device (--device belongs to send)'],
    [['export', '/nowhere/x.epub', '--set', '{}'],
      'export takes no --set (--set belongs to settings and app-settings)'],
  ];

  for (const [args, message] of CASES) {
    test(message, async () => {
      const json = await runCli([...args, '--json']);
      expect(json.exitCode).toBe(1);
      expect(JSON.parse(json.stdout)).toEqual({ ok: false, error: { code: 'usage', message } });
    });
  }

  test('a refused settings call writes nothing, even with a real script and a --set beside the foreign flag', async () => {
    const dir = mkdtempSync(join(SCRATCH, 'settings-'));
    const fountain = join(dir, 'Script.fountain');
    writeFileSync(fountain, 'INT. ROOM - DAY\n\nA beat.\n');
    const { stdout, exitCode } = await runCli([
      'settings', fountain, '--set', '{"justifyText":true}', '--device', 'kindle-1', '--json',
    ]);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error.code).toBe('usage');
    expect(existsSync(join(dir, 'Script.screepub.json'))).toBe(false);
  });
});

describe('the conversion path', () => {
  test('a verb\'s flag is refused as usage, not ignored', async () => {
    for (const flag of ['--device', '--set', '--for', '--out']) {
      const { stdout, exitCode } = await runCli(['/nowhere/x.pdf', ...FLAGS[flag], '--json']);
      const answer = JSON.parse(stdout);
      expect(`${flag}: ${exitCode} ${answer.error.code} ${answer.error.message.includes(flag)}`)
        .toBe(`${flag}: 1 usage true`);
    }
  });

  test('--fountain on an input that is not a PDF is refused: only a PDF conversion writes a .fountain', async () => {
    for (const ext of ['.fountain', '.txt']) {
      const { stdout, exitCode } = await runCli([
        `/nowhere/x${ext}`, '--fountain', '/nowhere/y.fountain', '--json',
      ]);
      expect(exitCode).toBe(1);
      expect(JSON.parse(stdout).error).toEqual({
        code: 'usage',
        message: `a ${ext} input takes no --fountain (only a PDF conversion writes a .fountain)`,
      });
    }
    // An input of no supported kind still gets the refusal that says so,
    // not this one.
    const { stdout } = await runCli(['/nowhere/x.docx', '--fountain', '/nowhere/y.fountain', '--json']);
    expect(JSON.parse(stdout).error.code).toBe('unsupported-type');
  });

  test('--fountain beside --no-fountain is refused rather than one silently losing', async () => {
    const { stdout, exitCode } = await runCli([
      '/nowhere/x.pdf', '--fountain', '/nowhere/y.fountain', '--no-fountain', '--json',
    ]);
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout).error).toEqual({
      code: 'usage',
      message: 'pass --fountain or --no-fountain, not both (--no-fountain writes no .fountain for --fountain to name)',
    });
  });
});
