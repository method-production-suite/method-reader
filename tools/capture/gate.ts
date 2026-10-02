// Which engine calls the capture tool may make on the window's behalf.
//
// The gate allows only calls whose WHOLE SHAPE the window's own argv
// builders (desktop/ui/app.js) reproduce exactly — not calls that merely
// start with a recognised verb, or merely put -o somewhere inside the
// library. That is the difference between a real boundary and a checklist:
// an unknown flag, a repeated flag, a `--flag=value` form the window never
// writes, a missing constant flag like --library, or a relative path all
// fail closed, because none of them is a shape any argv.* builder can
// produce. A constant flag a builder adds tomorrow is allowed automatically,
// since the gate rebuilds the call with that same builder and compares; a
// new VARIABLE part (a flag that takes a path, say) is refused until the
// gate is taught to recognise it, because nothing here can guess whether an
// unfamiliar value is safe.
//
// Pure. tests/capture.test.ts builds its allowed cases, and most of its
// refused ones, by taking a call straight from the window's own argv
// builders and, for a refusal, adding the one bad flag or path the label
// names; the rest of the refused cases are calls no argv builder makes at
// all (a missing constant flag, an unknown verb). Either way the gate is
// checked against the real contract, not a copy of it.
//
// Besides the conversion itself it allows the result screen's reads: the
// devices poll, the demo book's routes and Kindle check, and the one write
// a card makes (the reader's device, into the capture's scratch config).
// Every one of them lists or reads; nothing that copies, sends, builds or
// ejects gets through, and run.ts pins the device seams at empty places.

import { isAbsolute, resolve, sep } from 'node:path';
// @ts-expect-error -- plain JS module, no types
import { FORCE_FLAG, argv } from '../../desktop/ui/app.js';

export interface GateContext {
  /** The one file the capture may convert. */
  demoPdf: string;
  /** The scratch library the engine writes into for the capture. */
  library: string;
}

export type GateAnswer = { allow: true } | { allow: false; reason: string };

const ALLOW: GateAnswer = { allow: true };

function inside(path: string | null | undefined, dir: string): boolean {
  if (!path || !isAbsolute(path)) return false;
  return resolve(path).startsWith(resolve(dir) + sep);
}

const valueAfter = (args: readonly string[], flag: string): string | null => {
  const i = args.indexOf(flag);
  return i === -1 ? null : (args[i + 1] ?? null);
};

const same = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length && a.every((x, i) => x === b[i]);

export function gateEngineCall(args: readonly string[], ctx: GateContext): GateAnswer {
  const [first] = args;
  if (first === undefined) return { allow: false, reason: 'an empty engine call' };
  const opts = valueAfter(args, '--options-json');

  if (same(args, argv.version())) return ALLOW;

  // The Convert page's library line (parity piece C) reads the app settings
  // every time the drop area draws. The READ only, whole: argv.appSettings()
  // with no --set writes nothing, and the capture points SCREEPUB_CONFIG_DIR
  // at a scratch folder besides. Any --set is a write and stays refused.
  if (same(args, argv.appSettings())) return ALLOW;

  // The result screen's device poll and its one-off full listing. The
  // capture points SCREEPUB_VOLUME_ROOTS at an empty scratch folder and the
  // reMarkable at a closed local port (run.ts), so these list nothing real.
  if (same(args, argv.devices()) || same(args, argv.devices({ quick: true }))) return ALLOW;

  // A card chosen on the result screen: written into the capture's own
  // scratch config folder, and only the reader's device, by name.
  for (const kind of ['kindle', 'apple', 'kobo', 'tolino', 'remarkable']) {
    if (same(args, argv.readerDevice(kind))) return ALLOW;
  }

  // Read-only questions about the demo book: its routes and its Kindle file.
  if (first === 'routes' && inside(args[1], ctx.library)
    && (same(args, argv.routes(args[1])) || same(args, argv.routes(args[1], { quick: true })))) return ALLOW;
  if (first === 'export' && inside(args[1], ctx.library) && same(args, argv.kindleCheck(args[1]))) return ALLOW;

  if (
    first === 'settings' &&
    inside(args[1], ctx.library) &&
    same(args, argv.settings(args[1], valueAfter(args, '--set')))
  )
    return ALLOW;

  if (
    isAbsolute(first) &&
    resolve(first) === resolve(ctx.demoPdf) &&
    same(args, argv.convert(first, { force: args.includes(FORCE_FLAG), optionsJson: opts }))
  )
    return ALLOW;

  const out = valueAfter(args, '-o');
  if (
    inside(first, ctx.library) &&
    inside(out, ctx.library) &&
    opts !== null &&
    same(args, argv.reconvert(first, out, opts))
  )
    return ALLOW;

  return { allow: false, reason: `${args.join(' ')}: not a call the window makes on the way to a picture` };
}
