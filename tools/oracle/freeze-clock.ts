// Preloaded into the TypeScript CLI by the oracle and the parity harness
// (`bun --preload tools/oracle/freeze-clock.ts src/cli.ts ...`), so two runs
// of the same input write the same bytes. Transitional, like the rest of
// tools/oracle/: it goes when the TypeScript does.
//
// Three clocks reach the books today, and this stops all three without a
// change to the engine:
//   - the EPUB's dcterms:modified and every zip entry's DOS date
//     (src/epub/build.ts, JSZip), which the AZW3 then copies (EXTH 106);
//   - the PalmDB header's creation and modification times
//     (src/mobi/palmdb.ts).
//
// The time is SOURCE_DATE_EPOCH (seconds, the reproducible-builds
// convention), which the Rust CLI honors the same way. A zip's DOS date is
// LOCAL time, so the caller also sets TZ=UTC.
//
// Only `new Date()` with no arguments and `Date.now()` are frozen. Every
// other use of Date (parsing, arithmetic on a given time) is untouched.

const DEFAULT_EPOCH_S = 1767225600; // 2026-01-01T00:00:00Z

const raw = process.env.SOURCE_DATE_EPOCH;
const seconds = raw !== undefined && raw !== '' ? Number(raw) : DEFAULT_EPOCH_S;
if (!Number.isFinite(seconds)) {
  throw new Error(`freeze-clock: SOURCE_DATE_EPOCH is not a number: ${raw}`);
}
const frozenMs = seconds * 1000;

const RealDate = Date;

class FrozenDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) super(frozenMs);
    else super(...(args as [string | number | Date]));
  }

  static now(): number {
    return frozenMs;
  }
}

(globalThis as { Date: DateConstructor }).Date = FrozenDate as unknown as DateConstructor;

export {};
