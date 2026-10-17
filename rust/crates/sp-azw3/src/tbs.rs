//! Trailing byte sequences: the bytes at the end of each text record that
//! say which table-of-contents entries touch it.
//!
//! Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/tbs.py and
//! the helpers it takes from mobi/utils.py (encode_tbs, encode_fvwi,
//! encode_trailing_data). Calibre is GPL-3, Copyright Kovid Goyal; this port
//! is licensed GPL-3.0-only (LICENSE-GPL-3.0) (see THIRD-PARTY-NOTICES.md).
//! Ports src/azw3/tbs.ts.
//!
//! tbs.py's own account: for each text record, find the index entries that
//! start, end, span or are contained in it; arrange them into "strands",
//! each a hierarchical progression from the top level down; encode the
//! strands as sequences, and the sequences as bytes. Calibre writes them
//! into every KF8 book with a TOC, and so does this.

use crate::index::encint;

/// One TOC entry as the TBS see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TbsEntry {
    pub index: usize,
    pub start: usize,
    pub length: usize,
    pub depth: usize,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Spans,
    Ends,
    Starts,
    Completes,
}

#[derive(Debug, Clone, Copy)]
struct Filled {
    entry: TbsEntry,
    action: Action,
}

fn fill(entry: TbsEntry, start_offset: i64, record_length: usize) -> Filled {
    let length_offset = start_offset + entry.length as i64;
    let past = length_offset > record_length as i64;
    let action = match (start_offset < 0, past) {
        (true, true) => Action::Spans,
        (true, false) => Action::Ends,
        (false, true) => Action::Starts,
        (false, false) => Action::Completes,
    };
    Filled { entry, action }
}

fn remove(entries: &mut Vec<Filled>, index: usize) {
    if let Some(at) = entries.iter().position(|e| e.entry.index == index) {
        entries.remove(at);
    }
}

fn populate_strand(parent: Filled, entries: &mut Vec<Filled>) -> Vec<Filled> {
    let mut ans = vec![parent];
    let child = entries
        .iter()
        .find(|c| c.entry.parent == Some(parent.entry.index))
        .copied();
    if let Some(child) = child {
        remove(entries, child.entry.index);
        ans.extend(populate_strand(child, entries));
    } else {
        // Siblings at the same depth with consecutive indices and the same
        // parent ride in one sequence (the 0b100 flag).
        let mut current = parent.entry.index;
        let mut siblings = Vec::new();
        for entry in entries.clone() {
            if entry.entry.depth == parent.entry.depth
                && entry.entry.parent == parent.entry.parent
                && entry.entry.index == current + 1
            {
                current += 1;
                remove(entries, entry.entry.index);
                if entries
                    .iter()
                    .any(|c| c.entry.parent == Some(entry.entry.index))
                {
                    siblings.extend(populate_strand(entry, entries));
                    break;
                }
                siblings.push(entry);
            }
        }
        ans.extend(siblings);
    }
    ans
}

/// depth → entries, in the order depths first appear.
type Strand = Vec<(usize, Vec<Filled>)>;

fn separate_strands(local: Vec<Filled>) -> Vec<Strand> {
    let mut out = Vec::new();
    let mut entries = local;
    while !entries.is_empty() {
        let top = entries.remove(0);
        let strand = populate_strand(top, &mut entries);
        let mut layers: Strand = Vec::new();
        for e in strand {
            match layers.iter_mut().find(|(d, _)| *d == e.entry.depth) {
                Some((_, v)) => v.push(e),
                None => layers.push((e.entry.depth, vec![e])),
            }
        }
        out.push(layers);
    }
    out
}

/// A sequence's flags and their values.
#[derive(Debug, Clone, Copy, Default)]
struct Extra {
    /// 0b1: spans, value 0.
    spans: bool,
    /// 0b10: the TBS type.
    kind: Option<u64>,
    /// 0b100: how many siblings.
    count: Option<usize>,
    /// 0b1000: counts back from the strand before.
    back: bool,
}

impl Extra {
    fn flags(&self) -> u64 {
        u64::from(self.spans)
            | (u64::from(self.kind.is_some()) << 1)
            | (u64::from(self.count.is_some()) << 2)
            | (u64::from(self.back) << 3)
    }
}

/// The index went negative in a later strand, which type 8 cannot say.
struct NegativeStrandIndex;

fn encode_strands(
    strands: &[Strand],
    tbs_type: u64,
) -> Result<Vec<(u64, Extra)>, NegativeStrandIndex> {
    let mut ans: Vec<(u64, Extra)> = Vec::new();
    let mut last_index: i64 = 0;
    let first = strands
        .first()
        .and_then(|s| s.first())
        .and_then(|(_, v)| v.first())
        .map(|f| f.entry.index);

    for strand in strands {
        let mut seqs: Vec<(u64, Extra)> = Vec::new();
        for (_, entries) in strand {
            let mut extra = Extra::default();
            let head = entries[0].entry;
            let tail = entries[entries.len() - 1];
            if tail.action == Action::Spans {
                extra.spans = true;
            }
            if Some(head.index) == first {
                extra.kind = Some(tbs_type);
            }
            if entries.len() > 1 {
                extra.count = Some(entries.len());
            }
            let mut index = head.index as i64 - head.parent.unwrap_or(0) as i64;
            if !ans.is_empty() && seqs.is_empty() {
                // A second or later strand: the index counts back from the
                // last entry of the strand before it.
                index = last_index - head.index as i64;
                if index < 0 {
                    if tbs_type == 5 {
                        index = -index;
                    } else {
                        return Err(NegativeStrandIndex);
                    }
                } else {
                    extra.back = true;
                }
            }
            last_index = tail.entry.index as i64;
            seqs.push((
                u64::try_from(index).expect("a non-negative TBS index"),
                extra,
            ));
        }
        // Of consecutive 'spans' sequences, only the last keeps the 0b1 flag.
        for i in 0..seqs.len().saturating_sub(1) {
            if seqs[i].1.spans && seqs[i + 1].1.spans {
                seqs[i].1.spans = false;
            }
        }
        ans.extend(seqs);
    }
    Ok(ans)
}

fn encode_tbs(val: u64, extra: &Extra, flag_size: u32) -> Vec<u8> {
    let flags = extra.flags();
    let packed = val * (1u64 << flag_size) + (flags & ((1u64 << flag_size) - 1));
    let mut out = encint(packed, true);
    if let Some(kind) = extra.kind {
        out.extend(encint(kind, true));
    }
    if let Some(count) = extra.count {
        out.push((count & 0xff) as u8);
    }
    if extra.spans {
        out.extend(encint(0, true));
    }
    out
}

fn sequences_to_bytes(seqs: &[(u64, Extra)]) -> Vec<u8> {
    // Only the first sequence has a 3-bit flag field; every later one could
    // need the 0b1000 flag.
    seqs.iter()
        .enumerate()
        .flat_map(|(i, (val, extra))| encode_tbs(*val, extra, if i == 0 { 3 } else { 4 }))
        .collect()
}

/// `raw` followed by its own length as a backward variable-width integer:
/// how a reader finds a trailing entry from the end of a record.
pub fn encode_trailing_data(raw: &[u8]) -> Vec<u8> {
    let mut lsize = 1;
    loop {
        let encoded = encint((raw.len() + lsize) as u64, false);
        if encoded.len() == lsize {
            return [raw, &encoded].concat();
        }
        lsize += 1;
    }
}

/// The TBS bytes for each text record, given each record's UNCOMPRESSED
/// length. Index i is text record i + 1.
pub fn trailing_byte_sequences(entries: &[TbsEntry], record_lengths: &[usize]) -> Vec<Vec<u8>> {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|e| e.start);
    let mut per_record: Vec<Vec<Strand>> = Vec::new();
    let mut record_start = 0usize;
    for &length in record_lengths {
        let next = record_start + length;
        let mut local = Vec::new();
        for e in &sorted {
            if e.start >= next {
                break;
            }
            if e.start + e.length <= record_start {
                continue;
            }
            local.push(fill(*e, e.start as i64 - record_start as i64, length));
        }
        per_record.push(separate_strands(local));
        record_start = next;
    }
    let all = |kind: u64| -> Result<Vec<Vec<u8>>, NegativeStrandIndex> {
        per_record
            .iter()
            .map(|strands| encode_strands(strands, kind).map(|s| sequences_to_bytes(&s)))
            .collect()
    };
    match all(8) {
        Ok(v) => v,
        Err(NegativeStrandIndex) => all(5).unwrap_or_else(|_| unreachable!("type 5 never refuses")),
    }
}
