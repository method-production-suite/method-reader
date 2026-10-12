// Regenerate every oracle dump (see dump.ts for what one holds).
//
//   bun tools/oracle/dump-all.ts [--committed-only] [--real <dir>]
//
// Committed fixtures (tests/fixtures/*.pdf) → rust/oracle/<stem>/, which is
// committed. Real scripts (root fixtures/*.pdf and *.fountain) →
// rust/oracle-real/<stem>/, which is gitignored and must stay so: a real
// script's words never reach a commit. In a worktree, which has no root
// fixtures/, the real scripts are looked for in the main checkout.
//
// Prints counts for the real scripts, never their names.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { dump } from './dump';

const REPO = resolve(import.meta.dir, '../..');
const INPUT = /\.(pdf|fountain)$/i;

function realFixturesDir(explicit?: string): string | null {
  if (explicit) return resolve(explicit);
  const here = join(REPO, 'fixtures');
  if (existsSync(here)) return here;
  try {
    // The main checkout is the parent of the shared .git folder.
    const common = execFileSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], {
      cwd: REPO,
      encoding: 'utf8',
    }).trim();
    const main = join(dirname(common), 'fixtures');
    if (existsSync(main)) return main;
  } catch {
    // not a git checkout: no real scripts
  }
  return null;
}

const stem = (name: string) => name.slice(0, name.length - extname(name).length);

async function dumpFolder(from: string, to: string, only: RegExp, sayNames: boolean): Promise<number> {
  const inputs = readdirSync(from).filter((f) => only.test(f)).sort();
  // Every dump folder cleared each time, so a fixture that was removed
  // leaves no dump. Folders only: files at the top (.gitattributes) stay.
  mkdirSync(to, { recursive: true });
  for (const d of readdirSync(to, { withFileTypes: true })) {
    if (d.isDirectory()) rmSync(join(to, d.name), { recursive: true, force: true });
  }
  let n = 0;
  for (const f of inputs) {
    n++;
    if (sayNames) process.stderr.write(`oracle: ${f}\n`);
    else process.stderr.write(`oracle: real script ${n} of ${inputs.length}\n`);
    await dump({ input: join(from, f), out: join(to, stem(f)) });
  }
  return n;
}

const argv = process.argv.slice(2);
const committedOnly = argv.includes('--committed-only');
const realAt = argv.indexOf('--real');
const realArg = realAt >= 0 ? argv[realAt + 1] : undefined;

const committed = await dumpFolder(join(REPO, 'tests/fixtures'), join(REPO, 'rust/oracle'), /\.pdf$/i, true);
process.stdout.write(`committed fixtures: ${committed} dumped into rust/oracle/\n`);

if (!committedOnly) {
  const real = realFixturesDir(realArg);
  if (real === null) {
    process.stdout.write('real scripts: none found (no root fixtures/ folder)\n');
  } else {
    const count = await dumpFolder(real, join(REPO, 'rust/oracle-real'), INPUT, false);
    process.stdout.write(`real scripts: ${count} dumped into rust/oracle-real/ (gitignored)\n`);
  }
}
process.exit(0);
