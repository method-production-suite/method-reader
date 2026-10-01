// A pre-filled bug report. Ported in 4e74939 from the Swift Mac app's
// Feedback type (deleted at piece F3; see it at tag v0.7.4), so this is the
// only copy.
//
// Pure, and deliberately in the window rather than in src/: nothing but a
// window has a browser to open, and the CLI has no use for a GitHub issue
// URL. Putting it in the engine and transpiling it here — the arrangement
// update-compare.js has — would be ceremony for one consumer.
//
// The window does not OPEN this. app.js does, because app.js is the only
// file that touches Tauri, and opening a URL is now a Tauri call.

/** The repository's new-issue endpoint. The capability scopes the window to
 *  exactly this repo (capabilities/default.json), so a URL built here that
 *  pointed anywhere else would be refused rather than followed. */
export const NEW_ISSUE_BASE = 'https://github.com/ssandweiss/screepub/issues/new';

/** A pre-filled issue: an instruction, an optional context block seeded by
 *  whatever went wrong, and an environment footer.
 *
 *  The footer is the point of the whole thing. A report that arrives without
 *  a version costs a round trip before anyone can even look, and the two
 *  versions are the two facts the person reporting is least likely to know
 *  offhand and least able to get wrong if the app fills them in.
 *
 *  Built with URLSearchParams rather than by hand. The Swift version had to
 *  patch a literal "+" to %2B afterwards, because URLComponents leaves it
 *  alone and a query parser then reads it as a space — so "C++" arrived as
 *  "C  ". URLSearchParams encodes it correctly to begin with, which is why
 *  this port does not carry that fix-up across: the bug it fixed does not
 *  exist here. */
export function newIssueUrl({ appVersion, osVersion, context = null, home = null }) {
  const head = '<!-- Describe the issue or suggestion. -->\n\n';
  const foot = `\n---\nScreepub ${appVersion} · ${osVersion}\n`;
  let block = '';
  if (typeof context === 'string' && context.trim() !== '') {
    const open = '\n**What happened:**\n';
    const room = MAX_BODY - head.length - foot.length - open.length - 1;
    let said = redact(context, home);
    if (said.length > room) said = `${said.slice(0, Math.max(0, room - 1))}…`;
    block = `${open}${said}\n`;
  }
  const body = `${head}${block}${foot}`;

  const query = new URLSearchParams({ body });
  return `${NEW_ISSUE_BASE}?${query.toString()}`;
}

/** About the longest body a report carries. The context is cut to fit; the
 *  footer with the versions never is. */
export const MAX_BODY = 2000;

/** A path's start: the home sign, a drive letter, or a bare separator, at the
 *  start of the text or after a space, a quote or a bracket. Not after a
 *  letter (so "and/or" is prose) or a colon (so a link is left alone). */
const PATH = /(?<=^|[\s'"`(\[])(?:~|[A-Za-z]:)?[\\/][^\r\n'"`<>|]*/g;

/** The file name at the start of `rest`: up to its extension when it has one
 *  (a name may hold spaces), else up to the first space. */
function nameLength(rest) {
  const withExt = /^[^\\/\r\n]*?\.[A-Za-z0-9]{1,8}(?=$|[\s,;:)\]]|\.(?:\s|$))/.exec(rest);
  if (withExt) return withExt[0].length;
  const word = /^[^\s]*/.exec(rest);
  return word[0].length;
}

/** The engine's sentence, safe to put in a public URL. Every absolute path
 *  (POSIX, ~/... or Windows) becomes <path>, keeping only its last
 *  segment's extension (<path>.epub), which says what kind of file without
 *  saying which: a file name is usually the script's title, and so is the
 *  library folder the book sits in. The home folder (`home` when the window
 *  knows it, else any /Users/name, /home/name or C:\Users\name) becomes ~
 *  first, which is all that is left of it when it stands alone. */
export function redact(message, home = null) {
  let out = String(message ?? '');
  if (typeof home === 'string' && home.trim().length > 1) {
    const h = home.replace(/[\\/]+$/, '');
    out = out.split(h).join('~');
  }
  out = out
    .replace(/(?<![\w~])\/(?:Users|home)\/[^/\s'"`]+/g, '~')
    .replace(/(?<![\w~])[A-Za-z]:\\Users\\[^\\\s'"`]+/g, '~');
  return out.replace(PATH, (path) => {
    const at = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    const rest = path.slice(at + 1);
    const n = nameLength(rest);
    const ext = /\.[A-Za-z0-9]{1,8}$/.exec(rest.slice(0, n))?.[0] ?? '';
    return `<path>${ext}${rest.slice(n)}`;
  });
}

/** What the window knows about the machine it is on, for the footer. The
 *  Swift read ProcessInfo; a webview has only the user agent's platform,
 *  which is coarser. Coarse and honest beats precise and invented: this is
 *  a hint for triage, not a diagnostic. */
export function osLabel(platform) {
  const said = String(platform ?? '').trim();
  return said === '' ? 'unknown platform' : said;
}
