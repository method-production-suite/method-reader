// The Beta badge: one quiet mark beside a thing that works in our tests but
// has not been tried widely on real devices or apps. Four things carry it:
// the FDX row in Save a file, the two picture switches in Tune, Eject on
// Windows and Linux, and the device-request and route-report buttons. It is
// built here and nowhere else, so every one of them says the same words.
//
// Real text, not a colour: "Beta" is read out as part of the line it sits
// on, and the title and aria-description say what it means. Always BESIDE
// the thing it qualifies, never inside its button or label, so a control's
// name stays what it does (the asterisk's rule, send-panel.js).
import { el } from './dom.js';

export const BETA_LABEL = 'Beta';
export const BETA_NOTE = 'New: works in our tests, not yet tried widely on real devices or apps.';

export function betaBadge() {
  return el('span', { class: 'beta-badge', title: BETA_NOTE, 'aria-description': BETA_NOTE }, BETA_LABEL);
}
