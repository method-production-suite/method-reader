// The one way the engine parses Fountain. Every parse goes through here.
//
// fountain-js 1.2.4 keeps `Lexer.lastLineWasDualDialogue` as a STATIC and
// never resets it. It lexes bottom-up, so a script whose first speech is the
// right half of a dual exchange (`NAME ^`) ends its parse with the flag set,
// and the NEXT parse in the process wraps its last speech in a dual exchange
// that then drops it. One conversion parses the same .fountain several times
// (EPUB, MOBI, FDX, the image notes), so without this reset its outputs could
// disagree. tests/fountain-parse.test.ts pins it and fails if anything in
// src/ calls `new Fountain()` directly.

import { Fountain, Lexer, type Script } from 'fountain-js';

export function parseFountain(fountainText: string): Script {
  (Lexer as unknown as { lastLineWasDualDialogue: boolean }).lastLineWasDualDialogue = false;
  return new Fountain().parse(fountainText, true);
}
