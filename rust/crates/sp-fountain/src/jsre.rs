//! Regular expressions with ECMAScript semantics.
//!
//! fountain-js IS a set of regexes (`node_modules/fountain-js/dist/rules.js`),
//! and they lean on what the Rust regex crates do not do or do differently:
//! variable-length lookbehind (`(?<!(?:BLACK|OUT)\.|TO:\s*)`), backreferences
//! to named groups, JavaScript's own `\s` and `.`, ASCII-only case folding,
//! and leftmost-first backtracking order, which decides what a capture holds.
//! Translating each pattern by hand into something else would be fifteen
//! chances to change a token. So the patterns are copied verbatim and run
//! here, on a small backtracking matcher that follows the ECMAScript spec's
//! own matcher semantics (ES2024 §22.2.2) for the syntax they use.
//!
//! Positions are `char` indices, not UTF-16 code units. The patterns this
//! port runs never count characters except spaces, so the two agree on every
//! answer; `exec_at` and friends work on `&[char]` and convert at the edges.
//!
//! Supported: literals and escapes, `.`, classes with ranges and `\d\w\s`
//! (and negations), `^ $` (with `m`), `\b \B`, capturing, named and
//! non-capturing groups, alternation, greedy and lazy quantifiers including
//! `{n,m}`, lookahead and lookbehind (both polarities), numbered and named
//! backreferences, and the `i` and `m` flags. `g` and `y` are what the
//! caller does with `exec_at` and `find_from`, as in JavaScript.

use std::cell::Cell;

use crate::js::{is_line_terminator, is_space, is_word};

/// Most matcher steps one search may take before it gives up. fountain-js's
/// patterns backtrack exponentially on a few crafted inputs (a title-page
/// value ending in a separator that `.` cannot cross, in V8 as here); real
/// scripts use far fewer steps per search. A search over budget fails and
/// is counted in [`trips`], so a caller can refuse instead of hanging.
const STEP_BUDGET: u64 = 200_000_000;

/// Bytes of stack one search may use when the caller set no budget of its
/// own ([`with_stack_budget`]): a quarter of std's 2 MiB thread default,
/// the smallest stack this crate is called on.
const DEFAULT_STACK_BUDGET: usize = 512 * 1024;

thread_local! {
    static STACK_BUDGET: Cell<usize> = const { Cell::new(DEFAULT_STACK_BUDGET) };
    static TRIPS: Cell<u64> = const { Cell::new(0) };
}

/// How many searches on this thread have run out of steps or stack. The
/// matcher recurses once per repetition of a group, so a long enough run
/// (blank lines, lyrics) would otherwise overflow the stack, which aborts
/// the process: no `catch_unwind` sees a stack overflow.
pub fn trips() -> u64 {
    TRIPS.with(Cell::get)
}

/// Runs `f` with each search on this thread allowed `bytes` of stack. The
/// caller promises the thread has that much to spare.
pub fn with_stack_budget<T>(bytes: usize, f: impl FnOnce() -> T) -> T {
    let before = STACK_BUDGET.with(|b| b.replace(bytes));
    let out = f();
    STACK_BUDGET.with(|b| b.set(before));
    out
}

/// Where the stack is now: the address of a local. Stacks grow down on
/// every platform Method Reader ships on.
#[inline(never)]
fn stack_here() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// Capture slots: `[0]` is the whole match, `[n]` group `n`, as char ranges.
pub type Caps = Vec<Option<(usize, usize)>>;

#[derive(Debug, Clone)]
enum ClassItem {
    Ch(char),
    Range(char, char),
    Space(bool),
    Word(bool),
    Digit(bool),
}

#[derive(Debug, Clone)]
struct Class {
    negated: bool,
    items: Vec<ClassItem>,
}

impl Class {
    fn matches_raw(&self, c: char) -> bool {
        self.items.iter().any(|it| match *it {
            ClassItem::Ch(x) => x == c,
            ClassItem::Range(a, b) => a <= c && c <= b,
            ClassItem::Space(yes) => is_space(c) == yes,
            ClassItem::Word(yes) => is_word(c) == yes,
            ClassItem::Digit(yes) => c.is_ascii_digit() == yes,
        })
    }
}

#[derive(Debug, Clone)]
struct Repeat {
    node: Node,
    min: usize,
    max: Option<usize>,
    greedy: bool,
    /// The capture indices inside the repeated node, cleared each iteration.
    caps: (usize, usize),
}

#[derive(Debug, Clone)]
enum Node {
    Empty,
    Char(char),
    Any,
    Class(Class),
    Start,
    End,
    WordBoundary(bool),
    Group(Option<usize>, Box<Node>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat(Box<Repeat>),
    Look {
        behind: bool,
        negate: bool,
        node: Box<Node>,
    },
    BackRef(usize),
    NamedRef(String),
}

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct Regex {
    root: Node,
    ncaps: usize,
    names: Vec<(String, usize)>,
    icase: bool,
    multiline: bool,
}

struct Parser<'a> {
    p: &'a [char],
    i: usize,
    ncaps: usize,
    names: Vec<(String, usize)>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.p.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn starts(&self, s: &str) -> bool {
        let want: Vec<char> = s.chars().collect();
        self.p[self.i..].starts_with(&want)
    }

    fn alt(&mut self) -> Node {
        let mut alts = vec![self.concat()];
        while self.eat('|') {
            alts.push(self.concat());
        }
        if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            Node::Alt(alts)
        }
    }

    fn concat(&mut self) -> Node {
        let mut nodes = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let caps_before = self.ncaps;
            let atom = self.atom();
            let node = self.quantified(atom, caps_before);
            nodes.push(node);
        }
        match nodes.len() {
            0 => Node::Empty,
            1 => nodes.pop().unwrap(),
            _ => Node::Concat(nodes),
        }
    }

    /// `{n}`, `{n,}` or `{n,m}` at the cursor, if it is one; a `{` that is
    /// not a quantifier is a literal (Annex B).
    fn braces(&mut self) -> Option<(usize, Option<usize>)> {
        let save = self.i;
        if !self.eat('{') {
            return None;
        }
        let num = |me: &mut Self| -> Option<usize> {
            let start = me.i;
            while me.peek().is_some_and(|c| c.is_ascii_digit()) {
                me.i += 1;
            }
            if me.i == start {
                return None;
            }
            me.p[start..me.i].iter().collect::<String>().parse().ok()
        };
        let Some(min) = num(self) else {
            self.i = save;
            return None;
        };
        let max = if self.eat(',') {
            if self.peek() == Some('}') {
                None
            } else if let Some(m) = num(self) {
                Some(m)
            } else {
                self.i = save;
                return None;
            }
        } else {
            Some(min)
        };
        if !self.eat('}') {
            self.i = save;
            return None;
        }
        Some((min, max))
    }

    fn quantified(&mut self, atom: Node, caps_before: usize) -> Node {
        let (min, max) = match self.peek() {
            Some('*') => {
                self.i += 1;
                (0, None)
            }
            Some('+') => {
                self.i += 1;
                (1, None)
            }
            Some('?') => {
                self.i += 1;
                (0, Some(1))
            }
            Some('{') => match self.braces() {
                Some(q) => q,
                None => return atom,
            },
            _ => return atom,
        };
        let greedy = !self.eat('?');
        Node::Repeat(Box::new(Repeat {
            node: atom,
            min,
            max,
            greedy,
            caps: (caps_before + 1, self.ncaps + 1),
        }))
    }

    fn atom(&mut self) -> Node {
        let c = self.peek().expect("atom at end of pattern");
        self.i += 1;
        match c {
            '^' => Node::Start,
            '$' => Node::End,
            '.' => Node::Any,
            '[' => Node::Class(self.class()),
            '(' => self.group(),
            '\\' => self.escape(),
            '{' => {
                // Not a quantifier where an atom stands: a literal brace.
                Node::Char('{')
            }
            other => Node::Char(other),
        }
    }

    fn group(&mut self) -> Node {
        let node = if self.eat('?') {
            if self.eat(':') {
                Node::Group(None, Box::new(self.alt()))
            } else if self.eat('=') {
                Node::Look {
                    behind: false,
                    negate: false,
                    node: Box::new(self.alt()),
                }
            } else if self.eat('!') {
                Node::Look {
                    behind: false,
                    negate: true,
                    node: Box::new(self.alt()),
                }
            } else if self.starts("<=") || self.starts("<!") {
                self.i += 1;
                let negate = self.eat('!');
                if !negate {
                    self.i += 1;
                }
                Node::Look {
                    behind: true,
                    negate,
                    node: Box::new(self.alt()),
                }
            } else if self.eat('<') {
                let start = self.i;
                while self.peek() != Some('>') {
                    self.i += 1;
                }
                let name: String = self.p[start..self.i].iter().collect();
                self.i += 1;
                self.ncaps += 1;
                let idx = self.ncaps;
                self.names.push((name, idx));
                Node::Group(Some(idx), Box::new(self.alt()))
            } else {
                panic!("unsupported group syntax in pattern");
            }
        } else {
            self.ncaps += 1;
            let idx = self.ncaps;
            Node::Group(Some(idx), Box::new(self.alt()))
        };
        assert!(self.eat(')'), "unclosed group in pattern");
        node
    }

    fn hex(&mut self, n: usize) -> Option<char> {
        let s: String = self.p.get(self.i..self.i + n)?.iter().collect();
        let v = u32::from_str_radix(&s, 16).ok()?;
        self.i += n;
        char::from_u32(v)
    }

    /// An escape that means one character, shared by atoms and classes.
    fn char_escape(&mut self, c: char) -> char {
        match c {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{0B}',
            'f' => '\u{0C}',
            '0' => '\0',
            'u' => self.hex(4).unwrap_or('u'),
            'x' => self.hex(2).unwrap_or('x'),
            other => other,
        }
    }

    fn escape(&mut self) -> Node {
        let c = self.peek().expect("escape at end of pattern");
        self.i += 1;
        let class = |item| {
            Node::Class(Class {
                negated: false,
                items: vec![item],
            })
        };
        match c {
            'd' => class(ClassItem::Digit(true)),
            'D' => class(ClassItem::Digit(false)),
            'w' => class(ClassItem::Word(true)),
            'W' => class(ClassItem::Word(false)),
            's' => class(ClassItem::Space(true)),
            'S' => class(ClassItem::Space(false)),
            'b' => Node::WordBoundary(true),
            'B' => Node::WordBoundary(false),
            'k' if self.peek() == Some('<') => {
                self.i += 1;
                let start = self.i;
                while self.peek() != Some('>') {
                    self.i += 1;
                }
                let name: String = self.p[start..self.i].iter().collect();
                self.i += 1;
                Node::NamedRef(name)
            }
            '1'..='9' => {
                let mut n = c.to_digit(10).unwrap() as usize;
                while let Some(d) = self.peek().and_then(|d| d.to_digit(10)) {
                    n = n * 10 + d as usize;
                    self.i += 1;
                }
                Node::BackRef(n)
            }
            other => Node::Char(self.char_escape(other)),
        }
    }

    fn class(&mut self) -> Class {
        let negated = self.eat('^');
        let mut items = Vec::new();
        loop {
            let c = self.peek().expect("unclosed class in pattern");
            self.i += 1;
            if c == ']' {
                break;
            }
            let item = if c == '\\' {
                let e = self.peek().expect("escape at end of pattern");
                self.i += 1;
                match e {
                    'd' => ClassItem::Digit(true),
                    'D' => ClassItem::Digit(false),
                    'w' => ClassItem::Word(true),
                    'W' => ClassItem::Word(false),
                    's' => ClassItem::Space(true),
                    'S' => ClassItem::Space(false),
                    'b' => ClassItem::Ch('\u{08}'),
                    other => ClassItem::Ch(self.char_escape(other)),
                }
            } else {
                ClassItem::Ch(c)
            };
            // A range, unless either end is a class escape (Annex B) or the
            // dash is the last thing before `]`.
            if let ClassItem::Ch(lo) = item
                && self.peek() == Some('-')
                && self.p.get(self.i + 1).is_some_and(|&n| n != ']')
            {
                let save = self.i;
                self.i += 1;
                let hc = self.peek().unwrap();
                self.i += 1;
                let hi = if hc == '\\' {
                    let e = self.peek().unwrap();
                    self.i += 1;
                    if matches!(e, 'd' | 'D' | 'w' | 'W' | 's' | 'S') {
                        None
                    } else if e == 'b' {
                        Some('\u{08}')
                    } else {
                        Some(self.char_escape(e))
                    }
                } else {
                    Some(hc)
                };
                if let Some(hi) = hi {
                    items.push(ClassItem::Range(lo, hi));
                    continue;
                }
                self.i = save;
            }
            items.push(item);
        }
        Class { negated, items }
    }
}

fn resolve(node: &mut Node, names: &[(String, usize)]) {
    match node {
        Node::NamedRef(name) => {
            let idx = names
                .iter()
                .find(|(n, _)| n == name)
                .map(|&(_, i)| i)
                .expect("backreference to an unknown group name");
            *node = Node::BackRef(idx);
        }
        Node::Group(_, n) | Node::Look { node: n, .. } => resolve(n, names),
        Node::Concat(v) | Node::Alt(v) => v.iter_mut().for_each(|n| resolve(n, names)),
        Node::Repeat(r) => resolve(&mut r.node, names),
        _ => {}
    }
}

/// JavaScript's Canonicalize for a non-unicode, case-insensitive pattern:
/// simple uppercase, except that nothing outside ASCII folds INTO ASCII.
fn canon(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) if !u.is_ascii() => u,
        _ => c,
    }
}

type Cont<'k> = dyn FnMut(usize, &mut Caps) -> bool + 'k;

struct Ctx<'a> {
    re: &'a Regex,
    s: &'a [char],
    /// The stack where this search began, and how far below it it may go.
    base: usize,
    stack_budget: usize,
    steps: Cell<u64>,
    /// Out of steps or stack: every node fails from here on.
    tripped: Cell<bool>,
}

impl<'a> Ctx<'a> {
    fn new(re: &'a Regex, s: &'a [char]) -> Ctx<'a> {
        Ctx {
            re,
            s,
            base: stack_here(),
            stack_budget: STACK_BUDGET.with(Cell::get),
            steps: Cell::new(0),
            tripped: Cell::new(false),
        }
    }

    /// Charges one step; `true` once the search is out of steps or stack.
    fn exhausted(&self) -> bool {
        if self.tripped.get() {
            return true;
        }
        let steps = self.steps.get() + 1;
        self.steps.set(steps);
        if steps > STEP_BUDGET || self.base.saturating_sub(stack_here()) > self.stack_budget {
            self.tripped.set(true);
            TRIPS.with(|t| t.set(t.get() + 1));
            return true;
        }
        false
    }

    fn char_eq(&self, a: char, b: char) -> bool {
        a == b || (self.re.icase && canon(a) == canon(b))
    }

    fn class_matches(&self, cl: &Class, c: char) -> bool {
        let mut hit = cl.matches_raw(c);
        if !hit && self.re.icase {
            let lower = {
                let mut l = c.to_lowercase();
                match (l.next(), l.next()) {
                    (Some(x), None) => x,
                    _ => c,
                }
            };
            hit = cl.matches_raw(canon(c)) || (canon(lower) == canon(c) && cl.matches_raw(lower));
        }
        hit != cl.negated
    }

    /// Whether a one-character node matches `c`; `None` for any other node.
    fn single(&self, n: &Node, c: char) -> Option<bool> {
        match n {
            Node::Char(x) => Some(self.char_eq(*x, c)),
            Node::Any => Some(!is_line_terminator(c)),
            Node::Class(cl) => Some(self.class_matches(cl, c)),
            _ => None,
        }
    }

    fn is_single(n: &Node) -> bool {
        matches!(n, Node::Char(_) | Node::Any | Node::Class(_))
    }

    /// Match `n` at `i`, then the continuation. On `false`, `caps` is as it
    /// was on entry: every node restores what it changed before failing.
    fn node(&self, n: &Node, i: usize, caps: &mut Caps, k: &mut Cont<'_>) -> bool {
        if self.exhausted() {
            return false;
        }
        let s = self.s;
        match n {
            Node::Empty => k(i, caps),
            Node::Char(_) | Node::Any | Node::Class(_) => {
                i < s.len() && self.single(n, s[i]) == Some(true) && k(i + 1, caps)
            }
            Node::Start => {
                (i == 0 || (self.re.multiline && is_line_terminator(s[i - 1]))) && k(i, caps)
            }
            Node::End => {
                (i == s.len() || (self.re.multiline && is_line_terminator(s[i]))) && k(i, caps)
            }
            Node::WordBoundary(want) => {
                let a = i > 0 && is_word(s[i - 1]);
                let b = i < s.len() && is_word(s[i]);
                ((a != b) == *want) && k(i, caps)
            }
            Node::Group(None, sub) => self.node(sub, i, caps, k),
            Node::Group(Some(idx), sub) => {
                let idx = *idx;
                self.node(sub, i, caps, &mut |j, c: &mut Caps| {
                    let old = c[idx];
                    c[idx] = Some((i, j));
                    if k(j, c) {
                        true
                    } else {
                        c[idx] = old;
                        false
                    }
                })
            }
            Node::Concat(nodes) => self.seq(nodes, i, caps, k),
            Node::Alt(alts) => alts.iter().any(|a| self.node(a, i, caps, &mut *k)),
            Node::Repeat(r) => {
                if Self::is_single(&r.node) {
                    self.repeat_single(r, i, caps, k)
                } else {
                    self.repeat(r, r.min, r.max, i, caps, k)
                }
            }
            Node::Look {
                behind: false,
                negate,
                node,
            } => {
                let saved = caps.clone();
                let hit = self.node(node, i, caps, &mut |_, _| true);
                if *negate {
                    if hit {
                        *caps = saved;
                        return false;
                    }
                    k(i, caps)
                } else {
                    if !hit {
                        return false;
                    }
                    if k(i, caps) {
                        true
                    } else {
                        *caps = saved;
                        false
                    }
                }
            }
            Node::Look {
                behind: true,
                negate,
                node,
            } => {
                // JavaScript matches a lookbehind right to left; whether one
                // exists is the same question as whether some start j <= i
                // matches forward ending exactly at i. Captures inside a
                // lookbehind are not used by any pattern this port runs.
                let saved = caps.clone();
                let hit = (0..=i)
                    .rev()
                    .any(|j| self.node(node, j, caps, &mut |e, _| e == i));
                if *negate {
                    if hit {
                        *caps = saved;
                        return false;
                    }
                    k(i, caps)
                } else {
                    if !hit {
                        return false;
                    }
                    if k(i, caps) {
                        true
                    } else {
                        *caps = saved;
                        false
                    }
                }
            }
            Node::BackRef(idx) => {
                let (a, b) = caps.get(*idx).copied().flatten().unwrap_or((0, 0));
                let len = b - a;
                if i + len > s.len() {
                    return false;
                }
                if (0..len).all(|t| self.char_eq(s[a + t], s[i + t])) {
                    k(i + len, caps)
                } else {
                    false
                }
            }
            Node::NamedRef(_) => unreachable!("named references are resolved at compile"),
        }
    }

    fn seq(&self, nodes: &[Node], i: usize, caps: &mut Caps, k: &mut Cont<'_>) -> bool {
        match nodes.split_first() {
            None => k(i, caps),
            Some((first, rest)) => self.node(first, i, caps, &mut |j, c: &mut Caps| {
                self.seq(rest, j, c, &mut *k)
            }),
        }
    }

    fn repeat_single(&self, r: &Repeat, i: usize, caps: &mut Caps, k: &mut Cont<'_>) -> bool {
        let s = self.s;
        let ok = |j: usize| j < s.len() && self.single(&r.node, s[j]) == Some(true);
        let under_max = |n: usize| r.max.is_none_or(|m| n < m);
        if r.greedy {
            let mut n = 0;
            while under_max(n) && ok(i + n) {
                n += 1;
            }
            if n < r.min {
                return false;
            }
            (r.min..=n).rev().any(|c| k(i + c, caps))
        } else {
            for t in 0..r.min {
                if !ok(i + t) {
                    return false;
                }
            }
            let mut c = r.min;
            loop {
                if k(i + c, caps) {
                    return true;
                }
                if under_max(c) && ok(i + c) {
                    c += 1;
                } else {
                    return false;
                }
            }
        }
    }

    /// ES RepeatMatcher: `min` and `max` are what is LEFT to match.
    fn repeat(
        &self,
        r: &Repeat,
        min: usize,
        max: Option<usize>,
        i: usize,
        caps: &mut Caps,
        k: &mut Cont<'_>,
    ) -> bool {
        if max == Some(0) {
            return k(i, caps);
        }
        if min != 0 {
            return self.iteration(r, min, max, i, caps, k);
        }
        if !r.greedy {
            return k(i, caps) || self.iteration(r, min, max, i, caps, k);
        }
        self.iteration(r, min, max, i, caps, k) || k(i, caps)
    }

    /// One more iteration of `r` from `i`, its captures cleared first, and
    /// the rest of the repeat after it.
    fn iteration(
        &self,
        r: &Repeat,
        min: usize,
        max: Option<usize>,
        i: usize,
        caps: &mut Caps,
        k: &mut Cont<'_>,
    ) -> bool {
        let (a, b) = r.caps;
        let saved: Vec<_> = caps[a..b].to_vec();
        for slot in &mut caps[a..b] {
            *slot = None;
        }
        let hit = self.node(&r.node, i, caps, &mut |j, c: &mut Caps| {
            // An iteration past the minimum that consumed nothing fails.
            if min == 0 && j == i {
                return false;
            }
            self.repeat(r, min.saturating_sub(1), max.map(|m| m - 1), j, c, &mut *k)
        });
        if !hit {
            caps[a..b].copy_from_slice(&saved);
        }
        hit
    }
}

impl Regex {
    /// Compile a JavaScript pattern with flags (`i`, `m`; `g` and `y` are
    /// accepted and mean nothing here: they are how the caller loops).
    ///
    /// Panics on syntax this engine does not support, which can only be a
    /// pattern written into this crate's source.
    pub fn new(pattern: &str, flags: &str) -> Regex {
        let chars: Vec<char> = pattern.chars().collect();
        let mut p = Parser {
            p: &chars,
            i: 0,
            ncaps: 0,
            names: Vec::new(),
        };
        let mut root = p.alt();
        assert!(p.i == chars.len(), "unbalanced `)` in pattern {pattern}");
        resolve(&mut root, &p.names);
        Regex {
            root,
            ncaps: p.ncaps + 1,
            names: p.names,
            icase: flags.contains('i'),
            multiline: flags.contains('m'),
        }
    }

    /// The index of a named group.
    pub fn group_index(&self, name: &str) -> Option<usize> {
        self.names.iter().find(|(n, _)| n == name).map(|&(_, i)| i)
    }

    fn try_at(&self, ctx: &Ctx<'_>, start: usize, caps: &mut Caps) -> Option<Caps> {
        if ctx.tripped.get() {
            return None;
        }
        let mut out = None;
        ctx.node(&self.root, start, caps, &mut |end, c: &mut Caps| {
            let mut found = c.clone();
            found[0] = Some((start, end));
            out = Some(found);
            true
        });
        out
    }

    /// A match starting exactly at `start` (a sticky match).
    pub fn exec_at(&self, s: &[char], start: usize) -> Option<Caps> {
        if start > s.len() {
            return None;
        }
        let mut caps = vec![None; self.ncaps];
        self.try_at(&Ctx::new(self, s), start, &mut caps)
    }

    /// The first match starting at or after `from` (a global `exec`).
    pub fn find_from(&self, s: &[char], from: usize) -> Option<Caps> {
        let mut caps = vec![None; self.ncaps];
        let ctx = Ctx::new(self, s);
        (from..=s.len()).find_map(|i| self.try_at(&ctx, i, &mut caps))
    }

    /// `re.test(s)` for a pattern without `g`.
    pub fn test(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        self.find_from(&chars, 0).is_some()
    }

    /// `s.match(re)` for a pattern without `g`: every group as text,
    /// `None` for a group that did not take part.
    pub fn captures(&self, s: &str) -> Option<Vec<Option<String>>> {
        let chars: Vec<char> = s.chars().collect();
        let caps = self.find_from(&chars, 0)?;
        Some(texts(&chars, &caps))
    }

    /// `s.replace(re, f)` for a `g` pattern (`global`) or a plain one.
    /// `f` receives the match's groups as char ranges over `s`.
    pub fn replace_with(
        &self,
        s: &str,
        global: bool,
        mut f: impl FnMut(&[char], &Caps) -> String,
    ) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        let mut next = 0;
        let mut last = 0;
        while last <= chars.len() {
            let Some(caps) = self.find_from(&chars, last) else {
                break;
            };
            let (a, b) = caps[0].unwrap();
            if a >= next {
                out.extend(&chars[next..a]);
                out.push_str(&f(&chars, &caps));
                next = b;
            }
            if !global {
                break;
            }
            // An empty match moves on by one, as AdvanceStringIndex does.
            last = if b == a { b + 1 } else { b };
        }
        out.extend(&chars[next.min(chars.len())..]);
        out
    }

    /// `s.replace(re, literal)` where the literal has no `$` patterns.
    pub fn replace_all(&self, s: &str, with: &str) -> String {
        self.replace_with(s, true, |_, _| with.to_owned())
    }

    /// `s.split(re)` (`RegExp.prototype[@@split]`), with captures spliced
    /// in and an optional limit.
    pub fn split(&self, s: &str, limit: Option<usize>) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        self.split_chars(&chars, limit)
    }

    /// [`Regex::split`] over chars.
    pub fn split_chars(&self, chars: &[char], limit: Option<usize>) -> Vec<String> {
        let lim = limit.unwrap_or(usize::MAX);
        let mut out = Vec::new();
        if lim == 0 {
            return out;
        }
        let size = chars.len();
        if size == 0 {
            if self.exec_at(chars, 0).is_none() {
                out.push(String::new());
            }
            return out;
        }
        let mut p = 0;
        let mut q = 0;
        while q < size {
            let Some(caps) = self.exec_at(chars, q) else {
                q += 1;
                continue;
            };
            let e = caps[0].unwrap().1.min(size);
            if e == p {
                q += 1;
                continue;
            }
            out.push(chars[p..q].iter().collect());
            if out.len() == lim {
                return out;
            }
            p = e;
            for g in caps.iter().skip(1) {
                out.push(
                    g.map(|(a, b)| chars[a..b].iter().collect())
                        .unwrap_or_default(),
                );
                if out.len() == lim {
                    return out;
                }
            }
            q = p;
        }
        out.push(chars[p..size].iter().collect());
        out
    }
}

/// Every group of a match as text.
pub fn texts(chars: &[char], caps: &Caps) -> Vec<Option<String>> {
    caps.iter()
        .map(|g| g.map(|(a, b)| chars[a..b].iter().collect()))
        .collect()
}

/// Group `n` of a match as text, `None` if it did not take part.
pub fn group(chars: &[char], caps: &Caps, n: usize) -> Option<String> {
    caps.get(n)
        .copied()
        .flatten()
        .map(|(a, b)| chars[a..b].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::Regex;

    fn m(p: &str, f: &str, s: &str) -> Option<Vec<Option<String>>> {
        Regex::new(p, f).captures(s)
    }

    #[test]
    fn greedy_and_lazy() {
        assert_eq!(
            m("a(.*)b", "", "axxbxb").unwrap()[1].as_deref(),
            Some("xxbx")
        );
        assert_eq!(
            m("a(.*?)b", "", "axxbxb").unwrap()[1].as_deref(),
            Some("xx")
        );
        assert_eq!(
            m("^(a{2,3})", "", "aaaa").unwrap()[1].as_deref(),
            Some("aaa")
        );
    }

    #[test]
    fn lookaround_and_backrefs() {
        assert!(Regex::new("(?<!TO:\\s*)x", "").test("TOx"));
        assert!(!Regex::new("^(?:[A-Z ]+)(?<!TO:\\s*)$", "").test("CUT TO:  "));
        assert!(Regex::new("^(?<q>['\"]).*\\k<q>$", "").test("'abc'"));
        assert!(!Regex::new("^(?<q>['\"]).*\\k<q>$", "").test("'abc\""));
        assert!(Regex::new("^a(?=b)", "").test("ab"));
        assert!(!Regex::new("^a(?!b)", "").test("ab"));
    }

    #[test]
    fn case_folding_is_ascii_only() {
        assert!(Regex::new("^int", "i").test("INT"));
        // U+017F uppercases to ASCII 'S', which non-unicode JavaScript refuses.
        assert!(!Regex::new("^s", "i").test("\u{17F}"));
    }

    #[test]
    fn split_and_replace_follow_javascript() {
        let re = Regex::new("\\n", "");
        assert_eq!(re.split("a\nb\n", None), vec!["a", "b", ""]);
        assert_eq!(Regex::new("x*", "").replace_all("abc", "-"), "-a-b-c-");
        assert_eq!(
            Regex::new("(:)(.*)", "").split("k: v", Some(3)),
            vec!["k", ":", " v"]
        );
        assert_eq!(
            Regex::new("^\\s*|\\s*$", "gm").replace_all("Foo\n   Bar\n", ""),
            "Foo\nBar"
        );
    }
}
