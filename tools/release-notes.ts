// Facts for the release-notes drafter. Everything here is READ from the
// repo rather than recalled, because the drafter's failure mode is a
// confident claim nobody checked.
//
// Commit subjects are DATA, not instructions. They are author-controlled
// (a squash merge puts a contributor's PR title into the subject line), so
// they are truncated and stripped before they reach a model.

export interface CommitFact {
  subject: string;
  /** False once outside contributions land: the reviewer's cue to look harder. */
  ownerAuthored: boolean;
}

const MAX_SUBJECT = 200;

/** Argv-array spawnSync, never `sh -c` string interpolation: this file's
 *  whole point is that DATA (a tag name, a commit subject) never gets a
 *  chance to act as instructions, and a shell-interpolated sinceTag would
 *  undercut that on the very first line. */
function git(cwd: string, args: string[]): string {
  const proc = Bun.spawnSync(['git', ...args], { cwd });
  if (proc.exitCode !== 0) {
    throw new Error(`git ${args.join(' ')} failed: ${proc.stderr.toString().trim()}`);
  }
  return proc.stdout.toString();
}

/** Control characters and ANSI escapes out, length capped. Every control
 *  character here is an escape, never a literal: literals do not survive
 *  copy-paste, and this file was copied from a plan. */
export function sanitize(subject: string): string {
  const stripped = subject.replace(/[\x00-\x1F\x7F]/g, '');
  return stripped.length > MAX_SUBJECT ? stripped.slice(0, MAX_SUBJECT) : stripped;
}

/** The commit author, at minimum a Sam Sandweiss default, is meaningless
 *  once this AGPL repo is forked: a fork's own maintainer should read as
 *  the owner in their own clone, not as a permanent "look harder" flag on
 *  every commit they make. Read the repo's own git config; fall back to
 *  the literal only when nothing is configured at all. */
function currentGitUserName(cwd: string): string | undefined {
  try {
    const name = git(cwd, ['config', 'user.name']).trim();
    return name || undefined;
  } catch {
    return undefined;
  }
}

export function collectCommits(
  cwd: string,
  sinceTag: string,
  owner: string = currentGitUserName(cwd) ?? 'Sam Sandweiss',
): CommitFact[] {
  // %x1F separates fields, %x1E separates records: neither can appear in a
  // commit subject.
  const out = git(cwd, ['log', '--no-merges', '--format=%s%x1F%an%x1E', `${sinceTag}..HEAD`]);
  return out
    .split('\x1E')
    .map((rec) => rec.trim())
    .filter(Boolean)
    .map((rec) => {
      const [subject, author] = rec.split('\x1F');
      return { subject: sanitize(subject ?? ''), ownerAuthored: (author ?? '') === owner };
    })
    .reverse();
}

export interface Verdict {
  /** Registry entry number, e.g. "17" or "5a". */
  entry: string;
  title: string;
  /** The entry's `Device support:` line, if any. The drafter needs this
   *  sentence to write an honest caveat; a number and title cannot. */
  support: string;
  /** The after-ref `Device verdict:` sentence, present only on resolved
   *  entries. A before-ref entry only ever says "pending"; the sentence
   *  that replaced it — the one the closer needs — lives in the after
   *  ref, so `resolved` is built from there, not from the before list. */
  verdict?: string;
}

const ENTRY_HEADING = /^### ([0-9]+[a-z]?)\.\s*(.*)$/gm;
/** Any heading, entry or not. An entry's body ends at the NEXT one of
 *  these (## or ###) after its own, not at the next ENTRY heading:
 *  without this, the file's last ### entry swallows everything under the
 *  following non-entry "## ..." section (871 - 810 = 61 lines / ~4200
 *  characters on the live registry, attributed to an unrelated entry),
 *  and a stray "Device verdict: pending" mention under that section makes
 *  the entry ABOVE it look pending too. */
const ANY_HEADING = /^#{2,3} .*$/gm;

interface RegistryEntry {
  entry: string;
  title: string;
  body: string;
}

/** Every "### N. Title" entry, body clamped to the next heading of ANY
 *  level. Shared by parseVerdicts, diffVerdicts and changedRegistryEntries
 *  so all three agree on where one entry ends and the next section begins. */
function registryEntries(registry: string): RegistryEntry[] {
  const entryHeadings = [...registry.matchAll(ENTRY_HEADING)];
  const boundaries = [...registry.matchAll(ANY_HEADING)].map((m) => m.index!);
  return entryHeadings.map((match) => {
    const start = match.index!;
    const end = boundaries.find((b) => b > start) ?? registry.length;
    return { entry: match[1], title: match[2].trim(), body: registry.slice(start, end) };
  });
}

/** Markdown here hard-wraps a bullet's prose across indented continuation
 *  lines (docs/formatting-options-log.md wraps well short of 80 columns).
 *  Un-wrap each bullet into one logical line before matching a field with
 *  `$`, or a sentence that crosses a physical line break truncates at the
 *  break — the live bug that clipped entry 17's "Device support" sentence
 *  at "(Kindle" and would have clipped entry 5a's "Device verdict"
 *  sentence at "without the" (the actual line break in both, on the live
 *  file). Continuation = indented, non-blank, and not itself a new bullet
 *  or heading. */
function unwrapBullets(text: string): string {
  const lines = text.split('\n');
  const out: string[] = [];
  for (const raw of lines) {
    const startsNewBlock = /^-\s/.test(raw) || /^#{1,6}\s/.test(raw) || raw.trim() === '';
    if (!startsNewBlock && out.length > 0 && /^\s+\S/.test(raw)) {
      out[out.length - 1] += ' ' + raw.trim();
    } else {
      out.push(raw);
    }
  }
  return out.join('\n');
}

function supportLine(body: string): string {
  return /^-\s+\*\*Device support:\*\*\s*(.*)$/m.exec(unwrapBullets(body))?.[1] ?? '';
}

/** The bolded "Device verdict..." sentence itself, not the elaboration
 *  that may follow it in the same bullet. */
function verdictSentence(body: string): string {
  return /\*\*Device verdict[^*]*\*\*/.exec(unwrapBullets(body))?.[0] ?? '';
}

/** Every registry entry whose body still says a verdict is pending. */
export function parseVerdicts(registry: string): Verdict[] {
  return registryEntries(registry)
    .map(({ entry, title, body }) => ({
      entry,
      title,
      support: supportLine(body),
      pending: /Device verdict:\s*pending/i.test(unwrapBullets(body)),
    }))
    .filter((e) => e.pending)
    .map(({ entry, title, support }) => ({ entry, title, support }));
}

/**
 * What CHANGED between two refs, which is the only newsworthy part.
 * `opened` feeds the honesty section. `resolved` feeds the improvements and
 * the closer: a claim you could not make last release and can now is
 * exactly what a release note is for. Pending at both refs is in neither,
 * because it was not news this cycle and would otherwise repeat forever.
 */
export function diffVerdicts(before: string, after: string): { opened: Verdict[]; resolved: Verdict[] } {
  const beforeList = parseVerdicts(before);
  const wasPending = new Set(beforeList.map((v) => v.entry));
  const nowPending = parseVerdicts(after);
  const nowPendingIds = new Set(nowPending.map((v) => v.entry));

  const opened = nowPending.filter((v) => !wasPending.has(v.entry));

  const afterEntries = new Map(registryEntries(after).map((e) => [e.entry, e]));
  const resolved = [...wasPending]
    .filter((id) => !nowPendingIds.has(id))
    .map((id) => {
      const afterEntry = afterEntries.get(id);
      const body = afterEntry?.body ?? '';
      return {
        entry: id,
        title: afterEntry?.title ?? '',
        support: supportLine(body),
        verdict: verdictSentence(body),
      };
    });

  return { opened, resolved };
}

/**
 * Registry entries added or materially edited between two refs. This is the
 * primary change signal, not the FormatOptions interface: over the 0.5.0
 * range the registry caught five of six reader-visible changes while the
 * options interface caught one.
 */
export function changedRegistryEntries(before: string, after: string): { entry: string; title: string }[] {
  const old = new Map(registryEntries(before).map((e) => [e.entry, e.body.trim()]));
  return registryEntries(after)
    .filter((e) => old.get(e.entry) !== e.body.trim())
    .map(({ entry, title }) => ({ entry, title }));
}

/** Directories that never ship to a reader: docs, tests, CI config, and
 *  this tool's own home. A change confined to these should report
 *  `userVisible: false` and skip a drafting pass — this branch's own
 *  commits (tools/ + tests/ only) are the worked example. */
const NON_SHIPPING_PREFIXES = ['docs/', 'tests/', '.github/', 'tools/', 'site/', 'brand/'];

/**
 * Whether ANY changed path ships to a reader.
 *
 * A denylist, not an allowlist (`src/`, `desktop/`), on purpose: the two fail
 * differently. A denylist gap (a new non-shipping top-level directory not
 * yet listed) costs an occasional wasted drafting pass on nothing —
 * cheap, and it corrects itself the first time someone notices. An
 * allowlist gap (a new SHIPPING directory not yet listed) would silently
 * report false and skip drafting for a real reader-visible change — the
 * wrong failure for a gate whose false is a hard stop.
 */
export function isUserVisible(changedPaths: string[]): boolean {
  return changedPaths.some((p) => !NON_SHIPPING_PREFIXES.some((prefix) => p.startsWith(prefix)));
}

/** `bun tools/release-notes.ts <sinceTag>` prints the facts as JSON. */
if (import.meta.main) {
  const sinceTag = process.argv[2];
  if (!sinceTag) {
    console.error('usage: bun tools/release-notes.ts <sinceTag>');
    process.exit(2);
  }
  const cwd = process.cwd();
  const registryPath = 'docs/formatting-options-log.md';
  const before = git(cwd, ['show', `${sinceTag}:${registryPath}`]);
  const after = await Bun.file(registryPath).text();
  const { opened, resolved } = diffVerdicts(before, after);

  const changedPaths = git(cwd, ['diff', '--name-only', `${sinceTag}..HEAD`])
    .split('\n')
    .filter(Boolean);

  console.log(
    JSON.stringify(
      {
        sinceTag,
        commits: collectCommits(cwd, sinceTag),
        verdictsOpenedInRange: opened,
        verdictsResolvedInRange: resolved,
        registryChanges: changedRegistryEntries(before, after),
        userVisible: isUserVisible(changedPaths),
      },
      null,
      2,
    ),
  );
}
