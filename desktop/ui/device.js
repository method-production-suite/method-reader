// The reader's device, and what each one offers (spec 2026-10-01,
// decisions 3 to 10, 12 and 16). Pure: every rule the send panel draws is
// here, tested by tests/send-device.test.ts. The routes themselves are the
// engine's (send-routes.js checks them); this file only says which of them a
// person with this device should see first.
import {
  PROVEN, platformOf, undetectable, buttonLabel, isDeviceRoute, failureMessage, sentLine, isText,
} from './send-routes.js';

export { platformOf, undetectable };

export const QUESTION = 'What do you read on?';
export const LOOKING = 'Looking for every way to send it…';
export const OTHER_WAYS = 'Other ways';
export const DIFFERENT = 'Different device';
export const SEND_IT = 'Send it';
export const SAVE_A_FILE = 'Save a file';
/** The Read and Settings pages' button back to the result. */
export const SEND_BOOK = 'Send';

/** The cards, in the spec's order. `kind` is what the engine stores as
 *  `readerDevice` (src/settings/app.ts's READER_DEVICES). */
export const DEVICES = [
  { kind: 'kindle', name: 'Kindle', sub: null },
  { kind: 'apple', name: 'iPhone, iPad or Mac', sub: 'Apple Books' },
  { kind: 'kobo', name: 'Kobo', sub: null },
  { kind: 'tolino', name: 'tolino', sub: null },
  { kind: 'remarkable', name: 'reMarkable', sub: null },
];
export const KINDS = DEVICES.map((d) => d.kind);
export const MISSING = { kind: 'missing', name: 'My device isn’t here', sub: null };
/** The placeholder part 4 replaces with the request field. */
export const MISSING_LINE = 'Screepub does not know it yet. Tell us which one and we will look at adding it. '
  + 'Until then, the EPUB works on most readers.';

export const nameOf = (kind) => DEVICES.find((d) => d.kind === kind)?.name ?? 'reader';

/** `app-settings --json`'s readerDevice, or null for none, unknown or a
 *  refusal (the cards are asked again; nothing is written). */
export function readerDeviceFrom(answer) {
  return answer?.ok === true && KINDS.includes(answer.readerDevice) ? answer.readerDevice : null;
}

/** The first connected reader of `kind` in the watcher's list, or null. */
export function pluggedIn(kind, devices) {
  if (kind === 'apple' || !Array.isArray(devices)) return null;
  return devices.find((d) => d?.kind === kind) ?? null;
}

/** Built and code-tested, never run on that hardware here. The ledger's one
 *  proven pair is a Kindle on a Mac; Apple Books is a Mac app, and off a Mac
 *  the Apple card is only a save. */
export function unproven(kind, platform) {
  if (kind === 'apple') return false;
  return !(kind === PROVEN.kind && platformOf(platform) === PROVEN.platform);
}

const OS = { windows: 'Windows', linux: 'Linux' };

/** What pressing or hovering the asterisk says, or null when there is none. */
export function asteriskNote(kind, platform) {
  if (!unproven(kind, platform)) return null;
  const tail = 'yet. If it works for you, tell us.';
  if (kind === PROVEN.kind) {
    const where = OS[platformOf(platform)] ?? 'this kind of computer';
    return `Built and tested in code, never tried with a real Kindle on ${where} ${tail}`;
  }
  return `Built and tested in code, never tried on a real ${nameOf(kind)} ${tail}`;
}

export function cardsFor(platform, devices) {
  return [
    ...DEVICES.map((d) => ({ ...d, here: pluggedIn(d.kind, devices) !== null, unproven: unproven(d.kind, platform) })),
    { ...MISSING, here: false, unproven: false },
  ];
}

export const WEB_LINE = 'Amazon’s Send to Kindle opens beside the book’s folder. Drag the book into it: '
  + 'Screepub cannot upload it for you.';
export const PLUG_KINDLE = 'Or plug the Kindle in over USB and copy it across.';
export const APPLE_ELSEWHERE = 'Then put it in iCloud Drive, or email it to yourself, and open it in Books '
  + 'on your iPhone or iPad.';

export function plugLine(kind, platform) {
  if (kind === 'tolino') {
    const cannot = undetectable(platform);
    if (cannot !== null) return cannot;
  }
  if (kind === 'remarkable') {
    return 'Dock it over USB, with its USB web interface switched on, and this becomes Upload to reMarkable.';
  }
  const name = nameOf(kind);
  return `Plug your ${name} in over USB and this becomes Copy to ${name}.`;
}

const available = (shown, test) => shown.routes.find((r) => r.available && test(r)) ?? null;
const keyed = (key) => (r) => r.key === key;
const readerOf = (kind) => (r) => isDeviceRoute(r) && r.device?.kind === kind;

/** The calm screen for `device`: one brass action, the follow-ups that lead
 *  Other ways, the lines under the brass, and whether it carries the
 *  asterisk. `shown` is routesFrom()'s answer (null while it is on its way),
 *  `kindle` kindleCheckFrom()'s, which names the Kindle file on its button. */
export function offerFor({ device, platform, shown, kindle = null }) {
  if (!KINDS.includes(device)) return null;
  const base = { device, unproven: unproven(device, platform), brass: null, next: [], lines: [], waiting: false };
  if (!Array.isArray(shown?.routes)) return { ...base, waiting: true, lines: [LOOKING] };
  const act = (route) => (route === null ? null : { route, label: buttonLabel(route, kindle) });
  const save = act(available(shown, keyed('save-epub')));
  if (device === 'kindle') {
    const usb = available(shown, readerOf('kindle'));
    const web = act(available(shown, keyed('send-to-kindle')));
    const mail = act(available(shown, keyed('email-to-kindle')));
    if (usb !== null) return { ...base, brass: act(usb), next: [web, mail].filter(Boolean) };
    return { ...base, brass: web ?? save, next: [mail].filter(Boolean), lines: web ? [WEB_LINE, PLUG_KINDLE] : [PLUG_KINDLE] };
  }
  if (device === 'apple') {
    const books = platformOf(platform) === 'mac' ? available(shown, keyed('apple-books')) : null;
    if (books !== null) return { ...base, brass: act(books) };
    return { ...base, brass: save, lines: [APPLE_ELSEWHERE] };
  }
  const usb = available(shown, readerOf(device));
  if (usb !== null) return { ...base, brass: act(usb) };
  return { ...base, brass: save, lines: [plugLine(device, platform)] };
}

/** The reader kind a route row is about, connected or dimmed, or null. */
export function kindOfRoute(route) {
  if (isText(route?.device?.kind)) return route.device.kind;
  const key = route?.key ?? '';
  if (key.startsWith('device:')) return key.slice('device:'.length);
  return key === 'remarkable' ? 'remarkable' : null;
}

/** Other ways' "Send it" group: the offer's follow-ups first, then every
 *  other route the engine lists, in its order, dimmed ones included. Never
 *  the brass one, and never the two saves: "Save a file" holds those. */
export function sendItRows(shown, offer) {
  if (!Array.isArray(shown?.routes)) return [];
  const rows = shown.routes.filter((r) => r.key !== 'save-epub' && r.key !== 'save-kindle');
  const lead = (offer?.next ?? []).map((a) => a.route.id);
  const brass = offer?.brass?.route.id ?? null;
  return [
    ...lead.map((id) => rows.find((r) => r.id === id)).filter(Boolean),
    ...rows.filter((r) => r.id !== brass && !lead.includes(r.id)),
  ];
}

export const SAVE_TYPES = [
  { type: 'epub', label: 'EPUB', who: 'Most readers, Apple Books, and email.' },
  { type: 'kfx', label: 'KFX', who: 'Modern Kindles over USB, with the best layout.' },
  { type: 'azw3', label: 'AZW3', who: 'Older Kindles over USB.' },
  { type: 'mobi', label: 'MOBI', who: 'The oldest Kindles.' },
  { type: 'fountain', label: 'Fountain', who: 'The script as plain text, for screenwriting apps.' },
];
export const NEEDS_TOOLS = 'This computer cannot make one yet.';
export const SET_IT_UP = 'Set it up';
export const NO_FOUNTAIN = 'This book was made without a .fountain, so there is none to work from.';

/** The Save a file rows. `kindle` is the check's answer: the ladder's top
 *  rung says whether KFX is reachable, `builtBy` whether Calibre is there.
 *  Not known yet leaves every row live: the engine refuses in its own words. */
export function saveRows(script, kindle) {
  const fountain = isText(script?.fountainPath);
  return SAVE_TYPES.map((t) => {
    if ((t.type === 'mobi' || t.type === 'fountain') && !fountain) {
      return { ...t, available: false, fix: NO_FOUNTAIN, setup: false };
    }
    if (t.type === 'kfx' && kindle && kindle.extension !== 'kfx') return { ...t, available: false, fix: NEEDS_TOOLS, setup: true };
    if (t.type === 'azw3' && kindle?.builtBy === 'screepub') return { ...t, available: false, fix: NEEDS_TOOLS, setup: true };
    return { ...t, available: true, fix: null, setup: false };
  });
}

/** book-queue.js's turn for a save of `type`, and whether it also waits in
 *  the Kindle line (send-panel.js's inKindleLine). `export --for mobi` builds
 *  the MOBI without touching the book, so it only reads it. */
export function saveTurn(type) {
  return type === 'kfx' || type === 'azw3' || type === 'mobi' ? 'kindle' : 'copy';
}

/** What the panel says after a copy the engine called done. */
export function copiedState(title, device, sent) {
  const book = isText(title) ? title.trim() : 'The book';
  if (!isText(device?.volume)) return { line: sentLine(device, sent), bad: false, eject: null };
  return {
    line: `${book} is on your ${device.name}. Eject it before you unplug.`,
    bad: false,
    eject: { id: device.id, name: device.name, volume: device.volume },
  };
}

export const ejectLabel = (eject) => `Eject ${eject.name}`;

export function ejectOutcome(answer, eject) {
  if (answer?.ok === true) return { line: `Your ${eject.name} is ejected. You can unplug it now.`, bad: false, eject: null };
  if (answer?.error?.code === 'unknown-device') {
    return { line: `Your ${eject.name} is not plugged in any more.`, bad: false, eject: null };
  }
  return { line: failureMessage(answer), bad: true, eject };
}

export function ejectStillThere(eject, devices) {
  if (!Array.isArray(devices)) return true;
  return devices.some((d) => d?.volume === eject.volume);
}

export const HEADS_UP_MS = 8000;
export const headsUpLine = (title) => `Kindle plugged in. Copy ${title}?`;
export const HEADS_UP_COPY = 'Copy it';
export const HEADS_UP_LATER = 'Not now';

/** Which Kindle, if any, the heads-up offers. `seen` is the Kindle ids of
 *  the last list (null before the first, which only seeds); an id that
 *  leaves the list is forgotten, so plugging it in again is news again. */
export function headsUpFor(seen, devices, { open, onResult }) {
  if (!Array.isArray(devices)) return { seen, show: null };
  const kindles = devices.filter((d) => d?.kind === 'kindle');
  const ids = kindles.map((d) => d.id);
  if (seen === null) return { seen: ids, show: null };
  const fresh = kindles.find((d) => !seen.includes(d.id)) ?? null;
  return { seen: ids, show: open && !onResult ? fresh : null };
}

/** Whether this is a Kindle session (the build-ahead's `forKindle`): the
 *  reader said Kindle, or one is plugged in now. */
export function kindleSession(device, devices) {
  return device === 'kindle' || pluggedIn('kindle', devices) !== null;
}
