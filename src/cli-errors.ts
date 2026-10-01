// The CLI's error contract, separated from cli.ts so tests can exercise
// the mapping without spawning the binary (cli.ts runs main() on import).

export interface JsonError {
  code:
    | 'scanned'
    | 'not-screenplay'
    | 'unreadable'
    | 'password'
    | 'unsupported-type'
    | 'usage'
    | 'bad-options'
    | 'internal'
    // The library folder could not be opened or made.
    | 'library'
    // An output we could not write, as opposed to an input we could not read.
    | 'unwritable'
    // Device commands (piece B). Same contract, same stdout rule.
    | 'no-devices'
    | 'ambiguous-device'
    | 'unknown-device'
    | 'send-failed'
    | 'unsupported-file'
    // Settings and export verbs (piece D). Same contract, same stdout rule.
    | 'bad-settings'
    | 'export-failed'
    // The KFX plugin install (parity piece D). Same contract, same stdout rule.
    | 'kfx-install-failed'
    // Showing a file in the file manager (parity piece C).
    | 'reveal-failed'
    // The route verb (parity piece B): a route that cannot fire here (its
    // message is the row's fix), and one whose app, page or folder did not
    // open (its message names which).
    | 'route-unavailable'
    | 'route-failed';
  message: string;
}

/// Map a thrown conversion error to its contract code, or null for
/// anything unrecognized (the caller's catch-all owns those). Detection
/// is TYPED — pdf.js exceptions carry stable `name`s — never substring:
/// a path like ~/scripts/password-notes/x.pdf must not classify as a
/// password failure.
export function mapConversionError(err: unknown): JsonError | null {
  const name = (err as { name?: string })?.name;
  if (name === 'PasswordException') {
    return {
      code: 'password',
      message: 'this PDF is password-protected: remove the password first',
    };
  }
  if (name === 'InvalidPDFException') {
    return {
      code: 'unreadable',
      message: 'this file is not a readable PDF. It may be corrupt or mislabeled',
    };
  }
  const fsCode = (err as NodeJS.ErrnoException)?.code;
  if (fsCode === 'ENOENT' || fsCode === 'EISDIR' || fsCode === 'EACCES') {
    return { code: 'unreadable', message: `cannot read the input file (${fsCode})` };
  }
  return null;
}

/** A failure that already knows its contract code. Thrown by the device
 * command handlers, which must not import the printer: they return values or
 * throw this, and cli.ts is the only place that decides how it reaches the
 * user. */
export class CliError extends Error {
  constructor(
    readonly code: JsonError['code'],
    message: string,
  ) {
    super(message);
    this.name = 'CliError';
  }

  toJson(): JsonError {
    return { code: this.code, message: this.message };
  }
}

/** The message to report for an arbitrary throw.
 *
 * NOT `(err as Error).message`: a non-Error throw (a string, a rejected
 * promise carrying an object) yields undefined, and `JSON.stringify` DROPS an
 * undefined value — so `{"ok":false,"error":{"code":"send-failed"}}` reaches
 * the caller with no `message` at all, while `JsonError.message` is declared
 * required and the Tauri decoder rejects the object. One coercion, used at
 * every site that turns a caught unknown into a contract error. */
export function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** A filesystem answer that means "not there, not allowed, or no room", as
 * opposed to a bug in us. */
const WRITE_REFUSALS = new Set([
  'ENOENT', 'EACCES', 'EPERM', 'EEXIST', 'ENOSPC', 'EROFS', 'EISDIR', 'ENOTDIR',
]);

/** Whether a failed write is the filesystem refusing, and so `unwritable`:
 * the caller's to fix. Anything else is left to the caller's catch-all, so a
 * bug of ours still arrives as `internal` rather than dressed up as the
 * disk's fault. */
export function isWriteRefusal(err: unknown): boolean {
  const code = (err as NodeJS.ErrnoException)?.code;
  return code !== undefined && WRITE_REFUSALS.has(code);
}

/** Node's fs error codes, translated to a sentence fragment a person who has
 * never seen an errno can act on. Falls back to the raw message for
 * anything not covered, so a code this list has not met yet is still
 * reported rather than swallowed. Shared by app-settings (making the library
 * folder, saving the settings file) and a conversion's own output writes. */
export function plainReason(err: unknown): string {
  const code = (err as NodeJS.ErrnoException)?.code;
  if (code === 'EACCES' || code === 'EPERM') return 'permission denied';
  if (code === 'ENOTDIR') return 'part of that path is not a folder';
  // mkdirSync(recursive) on a path that ALREADY EXISTS AS A FILE (the exact
  // target, not a parent segment: that case is ENOTDIR above) fails EEXIST.
  if (code === 'EEXIST') return 'that is a file, not a folder';
  // The mirror image: a file written (or renamed) onto a path that is a
  // folder.
  if (code === 'EISDIR') return 'that is a folder, not a file';
  if (code === 'EROFS') return 'that location is read-only';
  if (code === 'ENOSPC') return 'no space left on the disk';
  if (code === 'ENAMETOOLONG') return 'that path is too long';
  return errorMessage(err);
}
