// The page as a sheet of paper on a desk: punched holes and brads down the
// binding margin, fixed to the window while the paper scrolls under them.
// Adapted from brand/components/page-frame.html and brand/components/brad.html.
//
// brand/README.md records that cross-file <use href="brad.html#brad"> does
// not resolve, so the symbols are defined once here, at the window's root,
// and every <use> points inside this document.
import { el } from './dom.js';
import { RELEASE } from './notes.js';

/** The surfaces the bar switches between. Notes is NOT one: it is the release
 *  notes for the version you are running, which is a thing you glance at
 *  rather than a place you go, and it is reached from the version stamp that
 *  names it. Exported so a test can read the bar without mounting it. */
export const SURFACES = [
  { id: 'convert', label: 'Convert' },
  { id: 'read', label: 'Read' },
  // The id stays `tune` and the label does not. The id is the module's name
  // and reaches nothing a reader sees; "Tune" was a metaphor for what is
  // plainly a settings screen. Renaming the module and the surface id would
  // churn focus plans, panel ids and a dozen test references to buy nothing.
  { id: 'tune', label: 'Settings' },
  // No Send: sending is the last step of converting, so it lives on
  // Convert's result (spec 2026-10-01, decision 1), and Read and Settings
  // carry a Send button back to it.
];

/** The revision stamp's words. "rev" is what a script's own revision mark
 *  says, and what the Mac app said here.
 *
 *  The number is the RELEASE's, taken from the generated notes module, not
 *  the engine's self-reported one. Two reasons. The engine's version and the
 *  app's are different numbers during the transition, so labelling the
 *  engine's "rev" would misname the build in the one place people paste into
 *  a bug report. And the stamp opens the notes for a version: taking both
 *  from `notes.js` makes the stamp and the sheet agree by construction rather
 *  than by two people remembering to change both. */
export function revLabel(version) {
  return `rev ${version}`;
}

// This window's own constant, character for character from brad.html's
// <defs>. It is the one piece of markup in the window that is not built from
// data, which is why it may be assigned rather than constructed.
const DEFS = `<defs>
  <radialGradient id="bradFace" cx="50%" cy="50%" r="58%" fx="33%" fy="27%">
    <stop offset="0" stop-color="var(--brass-specular)"/>
    <stop offset="0.17" stop-color="var(--brass-highlight)"/>
    <stop offset="0.45" stop-color="var(--brass)"/>
    <stop offset="0.76" stop-color="var(--brass-shadow)"/>
    <stop offset="1" stop-color="var(--brass-rim)"/>
  </radialGradient>
  <radialGradient id="bradSpec">
    <stop offset="0" stop-color="#fff" stop-opacity="0.88"/>
    <stop offset="0.55" stop-color="#fff" stop-opacity="0.2"/>
    <stop offset="1" stop-color="#fff" stop-opacity="0"/>
  </radialGradient>
  <radialGradient id="bradCast">
    <stop offset="0.52" stop-color="#000" stop-opacity="0.36"/>
    <stop offset="1" stop-color="#000" stop-opacity="0"/>
  </radialGradient>
  <radialGradient id="punchShade" cx="50%" cy="40%" r="54%">
    <stop offset="0.7" stop-color="#000" stop-opacity="0.6"/>
    <stop offset="1" stop-color="#000" stop-opacity="0"/>
  </radialGradient>
  <symbol id="brad" viewBox="0 0 100 100">
    <ellipse cx="53" cy="56" rx="47" ry="46" fill="url(#bradCast)"/>
    <circle cx="50" cy="50" r="41" fill="var(--brass-edge)"/>
    <circle cx="50" cy="50" r="39.5" fill="url(#bradFace)"/>
    <ellipse cx="63" cy="67" rx="15" ry="8" fill="var(--brass-bounce)" opacity="0.34" transform="rotate(-28 63 67)"/>
    <ellipse cx="38" cy="33" rx="17" ry="11" fill="url(#bradSpec)" transform="rotate(-28 38 33)"/>
    <circle cx="50" cy="50" r="39.5" fill="none" stroke="var(--brass-edge)" stroke-opacity="0.55" stroke-width="1.6"/>
  </symbol>
  <symbol id="punch" viewBox="0 0 100 100">
    <circle cx="50" cy="52.4" r="31.5" fill="var(--paper)"/>
    <circle cx="50" cy="50" r="30" fill="var(--ground)"/>
    <circle cx="50" cy="50" r="30" fill="url(#punchShade)"/>
  </symbol>
</defs>`;

/** Whether a surface's tab is on the bar at all, and whether it is the
 *  keyboard's one stop there. Pure, so the rule can be read and tested
 *  without a DOM.
 *
 *  A surface with nothing behind it is ABSENT rather than dimmed. Three
 *  greyed words advertise doors that do not open, and a disabled control is
 *  worse than useless to a screen reader: it is announced, and then refused.
 *
 *  The tabIndex half is the roving-tabindex rule the tablist already had:
 *  one stop for Tab, then the arrows move inside. It is answered here too
 *  because the two facts arrive separately — enable() and setSurface() are
 *  different calls — and a tab left at 0 while hidden is a focus stop
 *  pointing at nothing. */
export function tabPresence(available, selected) {
  if (!available) return { hidden: true, tabIndex: -1 };
  return { hidden: false, tabIndex: selected ? 0 : -1 };
}

function svgUse(cls, symbol) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('class', cls);
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', `#${symbol}`);
  svg.append(use);
  return svg;
}

export function mountFrame(root) {
  const defs = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  defs.setAttribute('width', '0');
  defs.setAttribute('height', '0');
  defs.setAttribute('aria-hidden', 'true');
  defs.setAttribute('class', 'defs');
  defs.innerHTML = DEFS;

  const handlers = [];
  let current = null;

  const tabs = SURFACES.map(({ id, label }) =>
    el('button', {
      type: 'button',
      class: 'tab',
      id: `tab-${id}`,
      role: 'tab',
      tabindex: '-1',
      'aria-selected': 'false',
      'aria-controls': `surface-${id}`,
      onclick: () => setSurface(id),
    }, label),
  );
  const tabFor = (id) => tabs[SURFACES.findIndex((s) => s.id === id)];

  // The tabs live ON the paper, inside the binding margin, so they line up
  // with the slugline below them and read as part of the page rather than as
  // chrome around it.
  // `data-tauri-drag-region` bare: a click on the bar ITSELF (the gaps
  // between tabs) moves the window; a click on a tab is still a tab, because
  // Tauri's drag script leaves anything clickable alone.
  const tablist = el('nav', {
    class: 'tabs', role: 'tablist', 'aria-label': 'Screepub', 'data-tauri-drag-region': '',
  }, tabs);
  const sheet = el('div', { class: 'sheet' }, tablist);
  const page = el('div', { class: 'page' }, sheet);
  const rail = el('div', { class: 'rail', 'aria-hidden': 'true' },
    svgUse('a', 'brad'), svgUse('b', 'punch'), svgUse('c', 'brad'));

  // A printer's mark at the foot of the paper, not a status bar: it belongs
  // to the page, so it is legible on it. It is a button because it opens the
  // notes for the version it names.
  const revHandlers = [];
  const stamp = el('button', {
    type: 'button',
    class: 'rev-stamp',
    onclick: () => { for (const handler of revHandlers) handler(); },
  }, revLabel(RELEASE.version));

  // A dead engine gets its own line rather than overwriting the stamp. The
  // stamp answers "which build is this?", which stays worth answering when
  // the engine is missing — arguably it is worth MORE then, since that is
  // exactly the moment someone files a report.
  const fault = el('p', { class: 'engine-fault' }, '');
  fault.hidden = true;

  // Beside the stamp, not on it: the stamp opens the notes for the version
  // you HAVE, and this does something about the one you could have. Hidden
  // until update-flow.js has something to say.
  const updateHandlers = [];
  const updateLabel = el('button', {
    type: 'button',
    class: 'rev-update',
    // A stable handle, not for styling: main.js checks
    // document.activeElement against this id so it can put the keyboard
    // back somewhere real when the label hides out from under it (a fresh
    // check saying "nothing newer" while the reader was standing on it).
    id: 'rev-update',
    hidden: true,
    onclick: () => { for (const handler of updateHandlers) handler(); },
  });
  const foot = el('div', { class: 'rev-foot' }, updateLabel, stamp);

  sheet.append(fault, foot);

  // The window has no title bar of its own (tauri.conf.json, titleBarStyle
  // "Overlay"), so without this there is nothing to grab. A transparent
  // strip at the top of the PAGE, not fixed to the viewport: see style.css's
  // .drag-strip for why a fixed strip covered controls scrolled under it.
  // As tall as the sheet's top margin so at rest it covers no control.
  const strip = el('div', { class: 'drag-strip', 'data-tauri-drag-region': '', 'aria-hidden': 'true' });

  root.append(defs, page, rail, strip);

  /** `disabled` is where a tab's availability is kept, so presence is derived
   *  from it rather than tracked twice. */
  function applyPresence(tab, selected) {
    const { hidden, tabIndex } = tabPresence(!tab.disabled, selected);
    tab.hidden = hidden;
    tab.tabIndex = tabIndex;
  }

  function setSurface(id) {
    if (id === current) return;
    current = id;
    for (const tab of tabs) {
      const mine = tab.id === `tab-${id}`;
      tab.setAttribute('aria-selected', mine ? 'true' : 'false');
      tab.classList.toggle('tab-on', mine);
      applyPresence(tab, mine);
    }
    for (const handler of handlers) handler(id);
  }

  // Left/Right move between surfaces, which is what a tablist owes a
  // keyboard: Tab alone walks into the surface and never comes back.
  // Home/End are the same contract's ends.
  tablist.addEventListener('keydown', (event) => {
    const step = event.key === 'ArrowRight' ? 1
      : event.key === 'ArrowLeft' ? -1
      : event.key === 'Home' ? 'first'
      : event.key === 'End' ? 'last'
      : 0;
    if (step === 0) return;
    const from = SURFACES.findIndex((s) => s.id === current);
    const order = typeof step === 'number'
      ? Array.from({ length: SURFACES.length - 1 },
        (_, hop) => SURFACES[(from + step * (hop + 1) + SURFACES.length * SURFACES.length)
          % SURFACES.length])
      : step === 'first' ? SURFACES : [...SURFACES].reverse();
    for (const next of order) {
      const button = tabFor(next.id);
      if (button.disabled) continue; // a dimmed surface is not a stop
      event.preventDefault();
      setSurface(next.id);
      button.focus();
      return;
    }
  });

  return {
    sheet,
    setSurface,
    onSurface: (handler) => handlers.push(handler),
    /** Called when the reader asks what changed in this version. */
    onRev: (handler) => revHandlers.push(handler),
    /** What the label beside the stamp says, or null to take it down. The
     *  words are update.js's (updateLabel); whether it is actually
     *  clickable right now is update.js's too (labelActionable) — most
     *  moments ("Downloading 0.8.0… 40%") are already under way, and a
     *  label that still looks like a button then is a small lie. Inert
     *  moments get aria-disabled and a class that drops the pointer cursor;
     *  the frame only shows what it is told. */
    setUpdateLabel: (words, { actionable } = {}) => {
      updateLabel.textContent = words ?? '';
      updateLabel.hidden = words === null || words === undefined;
      updateLabel.classList.toggle('rev-update-inert', !actionable);
      if (actionable) updateLabel.removeAttribute('aria-disabled');
      else updateLabel.setAttribute('aria-disabled', 'true');
    },
    /** Called when the reader clicks the label. */
    onUpdateClick: (handler) => updateHandlers.push(handler),
    /** The engine could not be started. Says so without taking the version
     *  off the page. */
    engineFailed: (message) => {
      fault.textContent = message;
      fault.hidden = false;
    },
    enable: (id, on) => {
      const tab = tabFor(id);
      const arriving = on && tab.hidden;
      tab.disabled = !on;
      applyPresence(tab, tab.id === `tab-${current}`);
      // Never leave the reader standing on a surface that has just gone
      // dark: the panel would be there with nothing in it.
      if (!on && current === id) setSurface('convert');
      // Only on the way IN, and only from actually absent. Re-running it on
      // every scriptChanged would replay the arrival each time a setting
      // moved, which is how a nice moment becomes a twitch.
      if (arriving) {
        tab.classList.remove('tab-arriving');
        void tab.offsetWidth; // restart the animation for a second script
        tab.classList.add('tab-arriving');
      }
    },
  };
}
