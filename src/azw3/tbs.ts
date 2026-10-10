// Trailing byte sequences: the bytes at the end of each text record that
// say which table-of-contents entries touch it.
//
// Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/tbs.py and
// the helpers it takes from mobi/utils.py (encode_tbs, encode_fvwi,
// encode_trailing_data). Calibre is GPL-3, Copyright Kovid Goyal; this port
// is part of Screepub, AGPL-3.0-or-later (see THIRD-PARTY-NOTICES.md).
//
// tbs.py's own account: for each text record, find the index entries that
// start, end, span or are contained in it; arrange them into "strands", each
// a hierarchical progression from the top level down; encode the strands as
// sequences, and the sequences as bytes. Calibre writes them into every KF8
// book with a TOC, and so does this.

import { concatBytes } from '../mobi/palmdb';
import { encint } from './index';

export interface TbsEntry {
  index: number;
  start: number;
  length: number;
  depth: number;
  parent?: number;
}

type Action = 'spans' | 'ends' | 'starts' | 'completes';
interface Filled extends TbsEntry {
  action: Action;
  lengthOffset: number;
  recordLength: number;
}

function fill(entry: TbsEntry, startOffset: number, recordLength: number): Filled {
  const lengthOffset = startOffset + entry.length;
  const action: Action = startOffset < 0
    ? (lengthOffset > recordLength ? 'spans' : 'ends')
    : (lengthOffset > recordLength ? 'starts' : 'completes');
  return { ...entry, action, lengthOffset, recordLength };
}

const sameParent = (a?: number, b?: number) => (a ?? null) === (b ?? null);

function populateStrand(parent: Filled, entries: Filled[]): Filled[] {
  const ans = [parent];
  const children = entries.filter((c) => c.parent === parent.index);
  if (children.length > 0) {
    const child = children[0];
    entries.splice(entries.indexOf(child), 1);
    ans.push(...populateStrand(child, entries));
  } else {
    // Siblings at the same depth with consecutive indices and the same
    // parent ride in one sequence (the 0b100 flag).
    let current = parent.index;
    const siblings: Filled[] = [];
    for (const entry of [...entries]) {
      if (entry.depth === parent.depth && sameParent(entry.parent, parent.parent) && entry.index === current + 1) {
        current++;
        entries.splice(entries.indexOf(entry), 1);
        const kids = entries.filter((c) => c.parent === entry.index);
        if (kids.length > 0) {
          siblings.push(...populateStrand(entry, entries));
          break;
        }
        siblings.push(entry);
      }
    }
    ans.push(...siblings);
  }
  return ans;
}

/** depth → entries, in the order depths first appear. */
type Strand = Map<number, Filled[]>;

function separateStrands(local: Filled[]): Strand[] {
  const out: Strand[] = [];
  let entries = local;
  while (entries.length > 0) {
    const top = entries[0];
    entries = entries.slice(1);
    const strand = populateStrand(top, entries);
    const layers: Strand = new Map();
    for (const e of strand) {
      if (!layers.has(e.depth)) layers.set(e.depth, []);
      layers.get(e.depth)!.push(e);
    }
    out.push(layers);
  }
  return out;
}

class NegativeStrandIndex extends Error {}

type Extra = Map<number, number | true>;

function encodeStrands(strands: Strand[], tbsType: number): [number, Extra][] {
  const ans: [number, Extra][] = [];
  let lastIndex = 0;
  const first = strands[0]?.values().next().value?.[0];

  for (const strand of strands) {
    const seqs: [number, Extra][] = [];
    for (const entries of strand.values()) {
      const extra: Extra = new Map();
      if (entries[entries.length - 1].action === 'spans') extra.set(0b1, 0);
      if (entries[0] === first) extra.set(0b10, tbsType);
      if (entries.length > 1) extra.set(0b100, entries.length);
      let index = entries[0].index - (entries[0].parent ?? 0);
      if (ans.length > 0 && seqs.length === 0) {
        // A second or later strand: the index counts back from the last
        // entry of the strand before it.
        index = lastIndex - entries[0].index;
        if (index < 0) {
          if (tbsType === 5) index = -index;
          else throw new NegativeStrandIndex();
        } else {
          extra.set(0b1000, true);
        }
      }
      lastIndex = entries[entries.length - 1].index;
      seqs.push([index, extra]);
    }
    // Of consecutive 'spans' sequences, only the last keeps the 0b1 flag.
    for (let i = 0; i + 1 < seqs.length; i++) {
      if (seqs[i][1].has(0b1) && seqs[i + 1][1].has(0b1)) seqs[i][1].delete(0b1);
    }
    ans.push(...seqs);
  }
  return ans;
}

function encodeTbs(val: number, extra: Extra, flagSize: number): Uint8Array {
  let flags = 0;
  for (const f of extra.keys()) flags |= f;
  let packed = val * 2 ** flagSize;
  for (let i = 0; i < flagSize; i++) packed += flags & (1 << i);
  const parts = [encint(packed)];
  if (extra.has(0b10)) parts.push(encint(extra.get(0b10) as number));
  if (extra.has(0b100)) parts.push(new Uint8Array([extra.get(0b100) as number]));
  if (extra.has(0b1)) parts.push(encint(extra.get(0b1) as number));
  return concatBytes(parts);
}

function sequencesToBytes(seqs: [number, Extra][]): Uint8Array {
  // Only the first sequence has a 3-bit flag field; every later one could
  // need the 0b1000 flag.
  return concatBytes(seqs.map(([val, extra], i) => encodeTbs(val, extra, i === 0 ? 3 : 4)));
}

/** `raw` followed by its own length as a backward variable-width integer:
 * how a reader finds a trailing entry from the end of a record. */
export function encodeTrailingData(raw: Uint8Array): Uint8Array {
  let lsize = 1;
  for (;;) {
    const encoded = encint(raw.length + lsize, false);
    if (encoded.length === lsize) return concatBytes([raw, encoded]);
    lsize++;
  }
}

/** The TBS bytes for each text record, given each record's UNCOMPRESSED
 * length. Index i is text record i + 1. */
export function trailingByteSequences(entries: readonly TbsEntry[], recordLengths: readonly number[]): Uint8Array[] {
  const sorted = [...entries].sort((a, b) => a.start - b.start);
  const perRecord: Strand[][] = [];
  let recordStart = 0;
  for (const length of recordLengths) {
    const next = recordStart + length;
    const local: Filled[] = [];
    for (const e of sorted) {
      if (e.start >= next) break;
      if (e.start + e.length <= recordStart) continue;
      local.push(fill(e, e.start - recordStart, length));
    }
    perRecord.push(separateStrands(local));
    recordStart = next;
  }
  const all = (type: number) => perRecord.map((strands) => sequencesToBytes(encodeStrands(strands, type)));
  try {
    return all(8);
  } catch (e) {
    if (e instanceof NegativeStrandIndex) return all(5);
    throw e;
  }
}
