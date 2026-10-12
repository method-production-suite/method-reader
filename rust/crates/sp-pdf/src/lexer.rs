//! pdf.js's content-stream Lexer and Parser, ported.
//!
//! Ported from pdf.js `src/core/parser.js` (Mozilla Foundation, Apache
//! License 2.0). A content stream is tokenized the way pdf.js tokenizes it,
//! errors and all, because what a malformed stream yields (where it stops,
//! which tokens survive) decides which text items exist.

use std::rc::Rc;

use crate::obj::{Dict, Obj, Stream, StreamSource, latin1};
use crate::ops::{OP_MAP, OpSpec};

/// pdf.js `specialChars`: 1 for whitespace, 2 for delimiters.
static SPECIAL_CHARS: [u8; 256] = {
    let mut t = [0u8; 256];
    t[0x00] = 1;
    t[0x09] = 1;
    t[0x0a] = 1;
    t[0x0c] = 1;
    t[0x0d] = 1;
    t[0x20] = 1;
    t[b'%' as usize] = 2;
    t[b'(' as usize] = 2;
    t[b')' as usize] = 2;
    t[b'/' as usize] = 2;
    t[b'<' as usize] = 2;
    t[b'>' as usize] = 2;
    t[b'[' as usize] = 2;
    t[b']' as usize] = 2;
    t[b'{' as usize] = 2;
    t[b'}' as usize] = 2;
    t
};

fn special(ch: i32) -> u8 {
    if (0..256).contains(&ch) {
        SPECIAL_CHARS[ch as usize]
    } else {
        0
    }
}

/// pdf.js `isWhiteSpace`.
pub fn is_white_space(ch: i32) -> bool {
    ch == 0x20 || ch == 0x09 || ch == 0x0d || ch == 0x0a
}

fn to_hex_digit(ch: i32) -> i32 {
    if (0x30..=0x39).contains(&ch) {
        return ch & 0x0f;
    }
    if (0x41..=0x46).contains(&ch) || (0x61..=0x66).contains(&ch) {
        return (ch & 0x0f) + 9;
    }
    -1
}

/// pdf.js's Stream: bytes and a cursor.
#[derive(Clone)]
pub struct ByteStream<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> ByteStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn get_byte(&mut self) -> i32 {
        if self.pos >= self.data.len() {
            return -1;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b as i32
    }
    pub fn peek_byte(&self) -> i32 {
        if self.pos >= self.data.len() {
            -1
        } else {
            self.data[self.pos] as i32
        }
    }
    pub fn peek_bytes(&self, n: usize) -> &'a [u8] {
        let end = (self.pos + n).min(self.data.len());
        &self.data[self.pos.min(end)..end]
    }
    pub fn skip(&mut self, n: isize) {
        let p = self.pos as isize + n;
        self.pos = p.max(0) as usize;
    }
    pub fn get_uint16(&mut self) -> i32 {
        let b0 = self.get_byte();
        let b1 = self.get_byte();
        if b0 == -1 || b1 == -1 {
            return -1;
        }
        (b0 << 8) + b1
    }
}

/// A lexer token: an object, a command (`Obj::Cmd`), or the end.
#[derive(Clone, Debug)]
pub enum Tok {
    Eof,
    Obj(Obj),
}

impl Tok {
    fn is_cmd(&self, c: &str) -> bool {
        matches!(self, Tok::Obj(Obj::Cmd(s)) if &**s == c)
    }
}

/// A format error: pdf.js throws, and the evaluator's `ignoreErrors`
/// handler stops reading the stream there.
#[derive(Debug)]
pub struct FormatError;

pub struct Lexer<'a> {
    pub stream: ByteStream<'a>,
    current_char: i32,
    known: bool,
    pub begin_inline_image_pos: isize,
}

impl<'a> Lexer<'a> {
    pub fn new(data: &'a [u8], known_commands: bool) -> Self {
        let mut l = Self {
            stream: ByteStream::new(data),
            current_char: -1,
            known: known_commands,
            begin_inline_image_pos: -1,
        };
        l.next_char();
        l
    }

    fn next_char(&mut self) -> i32 {
        self.current_char = self.stream.get_byte();
        self.current_char
    }

    fn peek_char(&self) -> i32 {
        self.stream.peek_byte()
    }

    fn get_number(&mut self) -> Result<f64, FormatError> {
        let mut ch = self.current_char;
        let mut divide_by: f64 = 0.0;
        let mut sign: f64 = 1.0;
        if ch == 0x2d {
            sign = -1.0;
            ch = self.next_char();
            if ch == 0x2d {
                ch = self.next_char();
            }
        } else if ch == 0x2b {
            ch = self.next_char();
        }
        if ch == 0x0a || ch == 0x0d {
            loop {
                ch = self.next_char();
                if ch != 0x0a && ch != 0x0d {
                    break;
                }
            }
        }
        if ch == 0x2e {
            divide_by = 10.0;
            ch = self.next_char();
        }
        if !(0x30..=0x39).contains(&ch) {
            if is_white_space(ch) || ch == 0x28 || ch == 0x3c || ch == -1 {
                return Ok(0.0);
            }
            return Err(FormatError);
        }
        let mut base_value = (ch - 0x30) as f64;
        loop {
            ch = self.next_char();
            if ch < 0 {
                break;
            }
            if (0x30..=0x39).contains(&ch) {
                let current_digit = (ch - 0x30) as f64;
                if divide_by != 0.0 {
                    divide_by *= 10.0;
                }
                base_value = base_value * 10.0 + current_digit;
            } else if ch == 0x2e {
                if divide_by == 0.0 {
                    divide_by = 1.0;
                } else {
                    break;
                }
            } else if ch == 0x2d {
                // "Badly formatted number: minus sign in the middle"
            } else {
                break;
            }
        }
        if divide_by != 0.0 {
            base_value /= divide_by;
        }
        Ok(sign * base_value)
    }

    fn get_string(&mut self) -> Vec<u8> {
        let mut num_paren = 1;
        let mut buf: Vec<u8> = Vec::new();
        let mut ch = self.next_char();
        loop {
            let mut char_buffered = false;
            let mut done = false;
            match ch {
                -1 => done = true,
                0x28 => {
                    num_paren += 1;
                    buf.push(b'(');
                }
                0x29 => {
                    num_paren -= 1;
                    if num_paren == 0 {
                        self.next_char();
                        done = true;
                    } else {
                        buf.push(b')');
                    }
                }
                0x5c => {
                    ch = self.next_char();
                    match ch {
                        -1 => done = true,
                        0x6e => buf.push(b'\n'),
                        0x72 => buf.push(b'\r'),
                        0x74 => buf.push(b'\t'),
                        0x62 => buf.push(0x08),
                        0x66 => buf.push(0x0c),
                        0x5c | 0x28 | 0x29 => buf.push(ch as u8),
                        0x30..=0x37 => {
                            let mut x = ch & 0x0f;
                            ch = self.next_char();
                            char_buffered = true;
                            if (0x30..=0x37).contains(&ch) {
                                x = (x << 3) + (ch & 0x0f);
                                ch = self.next_char();
                                if (0x30..=0x37).contains(&ch) {
                                    char_buffered = false;
                                    x = (x << 3) + (ch & 0x0f);
                                }
                            }
                            // String.fromCharCode keeps the low 16 bits; a
                            // three-digit octal tops out at 511, and the
                            // char then rides in a byte string as its low
                            // byte, the way a char code above 255 would be
                            // read back by every consumer here.
                            buf.push((x & 0xff) as u8);
                        }
                        0x0d => {
                            if self.peek_char() == 0x0a {
                                self.next_char();
                            }
                        }
                        0x0a => {}
                        _ => buf.push(ch as u8),
                    }
                }
                _ => buf.push(ch as u8),
            }
            if done {
                break;
            }
            if !char_buffered {
                ch = self.next_char();
            }
        }
        buf
    }

    fn get_name(&mut self) -> String {
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let mut ch = self.next_char();
            if ch < 0 || special(ch) != 0 {
                break;
            }
            if ch == 0x23 {
                ch = self.next_char();
                if special(ch) != 0 {
                    buf.push(b'#');
                    break;
                }
                let x = to_hex_digit(ch);
                if x != -1 {
                    let previous = ch;
                    ch = self.next_char();
                    let x2 = to_hex_digit(ch);
                    if x2 == -1 {
                        buf.push(b'#');
                        buf.push(previous as u8);
                        if special(ch) != 0 {
                            break;
                        }
                        buf.push(ch as u8);
                        continue;
                    }
                    buf.push(((x << 4) | x2) as u8);
                } else {
                    buf.push(b'#');
                    // ch may be -1 at the end of data: String.fromCharCode(-1)
                    // is U+FFFF, which no consumer here can match anyway.
                    if ch >= 0 {
                        buf.push(ch as u8);
                    }
                }
            } else {
                buf.push(ch as u8);
            }
        }
        latin1(&buf)
    }

    fn get_hex_string(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut ch = self.current_char;
        let mut first_digit = -1;
        loop {
            if ch < 0 {
                break;
            } else if ch == 0x3e {
                self.next_char();
                break;
            } else if special(ch) == 1 {
                ch = self.next_char();
                continue;
            } else {
                let digit = to_hex_digit(ch);
                if digit == -1 {
                    // ignored, with a warning
                } else if first_digit == -1 {
                    first_digit = digit;
                } else {
                    buf.push(((first_digit << 4) | digit) as u8);
                    first_digit = -1;
                }
                ch = self.next_char();
            }
        }
        if first_digit != -1 {
            buf.push((first_digit << 4) as u8);
        }
        buf
    }

    fn known(&self, s: &str) -> bool {
        self.known && OP_MAP.iter().any(|(k, _)| *k == s)
    }

    pub fn get_obj(&mut self) -> Result<Tok, FormatError> {
        let mut comment = false;
        let mut ch = self.current_char;
        loop {
            if ch < 0 {
                return Ok(Tok::Eof);
            }
            if comment {
                if ch == 0x0a || ch == 0x0d {
                    comment = false;
                }
            } else if ch == 0x25 {
                comment = true;
            } else if special(ch) != 1 {
                break;
            }
            ch = self.next_char();
        }
        match ch {
            0x30..=0x39 | 0x2b | 0x2d | 0x2e => return Ok(Tok::Obj(Obj::Num(self.get_number()?))),
            0x28 => return Ok(Tok::Obj(Obj::Str(Rc::from(self.get_string())))),
            0x2f => return Ok(Tok::Obj(Obj::Name(Rc::from(self.get_name().as_str())))),
            0x5b => {
                self.next_char();
                return Ok(cmd("["));
            }
            0x5d => {
                self.next_char();
                return Ok(cmd("]"));
            }
            0x3c => {
                let c2 = self.next_char();
                if c2 == 0x3c {
                    self.next_char();
                    return Ok(cmd("<<"));
                }
                return Ok(Tok::Obj(Obj::Str(Rc::from(self.get_hex_string()))));
            }
            0x3e => {
                let c2 = self.next_char();
                if c2 == 0x3e {
                    self.next_char();
                    return Ok(cmd(">>"));
                }
                return Ok(cmd(">"));
            }
            0x7b => {
                self.next_char();
                return Ok(cmd("{"));
            }
            0x7d => {
                self.next_char();
                return Ok(cmd("}"));
            }
            0x29 => {
                self.next_char();
                return Err(FormatError);
            }
            _ => {}
        }
        let mut s = String::new();
        s.push(ch as u8 as char);
        if !(0x20..=0x7f).contains(&ch) {
            let next = self.peek_char();
            if (0x20..=0x7f).contains(&next) {
                self.next_char();
                return Ok(Tok::Obj(Obj::Cmd(Rc::from(s.as_str()))));
            }
        }
        let mut known_found = self.known(&s);
        loop {
            ch = self.next_char();
            if ch < 0 || special(ch) != 0 {
                break;
            }
            let mut possible = s.clone();
            possible.push(ch as u8 as char);
            if known_found && !self.known(&possible) {
                break;
            }
            if s.len() == 128 {
                return Err(FormatError);
            }
            s = possible;
            known_found = self.known(&s);
        }
        match s.as_str() {
            "true" => return Ok(Tok::Obj(Obj::Bool(true))),
            "false" => return Ok(Tok::Obj(Obj::Bool(false))),
            "null" => return Ok(Tok::Obj(Obj::Null)),
            "BI" => self.begin_inline_image_pos = self.stream.pos as isize,
            _ => {}
        }
        Ok(Tok::Obj(Obj::Cmd(Rc::from(s.as_str()))))
    }
}

fn cmd(s: &str) -> Tok {
    Tok::Obj(Obj::Cmd(Rc::from(s)))
}

/// An inline image's bytes, its dictionary, and pdf.js's cache key for it.
pub struct Parser<'a> {
    pub lexer: Lexer<'a>,
    buf1: Tok,
    buf2: Tok,
    image_cache: Vec<(Vec<u8>, Rc<Stream>)>,
    image_id: u32,
    /// Per inline image stream (by pointer): its pdf.js cacheKey.
    pub inline_keys: Vec<(*const Stream, String)>,
}

impl<'a> Parser<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, FormatError> {
        let mut lexer = Lexer::new(data, true);
        let buf1 = lexer.get_obj()?;
        let buf2 = lexer.get_obj()?;
        Ok(Self {
            lexer,
            buf1,
            buf2,
            image_cache: Vec::new(),
            image_id: 0,
            inline_keys: Vec::new(),
        })
    }

    fn shift(&mut self) -> Result<(), FormatError> {
        if self.buf2.is_cmd("ID") {
            self.buf1 = std::mem::replace(&mut self.buf2, Tok::Obj(Obj::Null));
            // pdf.js sets buf2 to null here; it is never read before
            // makeInlineImage replaces it.
        } else {
            self.buf1 = std::mem::replace(&mut self.buf2, Tok::Eof);
            self.buf2 = self.lexer.get_obj()?;
        }
        Ok(())
    }

    pub fn get_obj(&mut self) -> Result<Tok, FormatError> {
        let buf1 = self.buf1.clone();
        self.shift()?;
        if let Tok::Obj(Obj::Cmd(c)) = &buf1 {
            match &**c {
                "BI" => return self.make_inline_image().map(|s| Tok::Obj(Obj::Stream(s))),
                "[" => {
                    let mut array = Vec::new();
                    while !self.buf1.is_cmd("]") && !matches!(self.buf1, Tok::Eof) {
                        match self.get_obj()? {
                            Tok::Obj(o) => array.push(o),
                            Tok::Eof => break,
                        }
                    }
                    if matches!(self.buf1, Tok::Eof) {
                        return Err(FormatError);
                    }
                    self.shift()?;
                    return Ok(Tok::Obj(Obj::Array(Rc::new(array))));
                }
                "<<" => {
                    let mut dict = Dict::default();
                    while !self.buf1.is_cmd(">>") && !matches!(self.buf1, Tok::Eof) {
                        let key = match &self.buf1 {
                            Tok::Obj(Obj::Name(n)) => n.clone(),
                            _ => {
                                self.shift()?;
                                continue;
                            }
                        };
                        self.shift()?;
                        if matches!(self.buf1, Tok::Eof) {
                            break;
                        }
                        if let Tok::Obj(o) = self.get_obj()? {
                            dict.set(&key, o);
                        }
                    }
                    if matches!(self.buf1, Tok::Eof) {
                        return Err(FormatError);
                    }
                    // allowStreams is false for content streams, so a
                    // following `stream` keyword just leaves the dict.
                    self.shift()?;
                    return Ok(Tok::Obj(Obj::Dict(Rc::new(dict))));
                }
                _ => return Ok(buf1),
            }
        }
        if let Tok::Obj(Obj::Num(n)) = &buf1
            && n.fract() == 0.0
            && n.is_finite()
        {
            if let Tok::Obj(Obj::Num(g)) = &self.buf1
                && g.fract() == 0.0
                && g.is_finite()
                && self.buf2.is_cmd("R")
            {
                let r = crate::obj::Ref {
                    num: *n as i32,
                    generation: *g as i32,
                };
                self.shift()?;
                self.shift()?;
                return Ok(Tok::Obj(Obj::Ref(r)));
            }
            return Ok(buf1);
        }
        Ok(buf1)
    }

    fn find_default_inline_stream_end(&mut self) -> usize {
        const E: i32 = 0x45;
        const I: i32 = 0x49;
        const SPACE: i32 = 0x20;
        const LF: i32 = 0x0a;
        const CR: i32 = 0x0d;
        const NUL: u8 = 0x00;
        let n = 15;
        let start_pos = self.lexer.stream.pos;
        let mut state = 0;
        let mut ch;
        let mut maybe_ei_pos: Option<usize> = None;
        loop {
            ch = self.lexer.stream.get_byte();
            if ch == -1 {
                break;
            }
            if state == 0 {
                state = if ch == E { 1 } else { 0 };
            } else if state == 1 {
                state = if ch == I { 2 } else { 0 };
            } else if ch == SPACE || ch == LF || ch == CR {
                maybe_ei_pos = Some(self.lexer.stream.pos);
                let following = self.lexer.stream.peek_bytes(n);
                let ii = following.len();
                if ii == 0 {
                    break;
                }
                for i in 0..ii {
                    let c = following[i];
                    if c == NUL && following.get(i + 1).copied() != Some(NUL) {
                        continue;
                    }
                    let c = c as i32;
                    if c != LF && c != CR && !(SPACE..=0x7f).contains(&c) {
                        state = 0;
                        break;
                    }
                }
                if state != 2 {
                    continue;
                }
                let window = self.lexer.stream.peek_bytes(5 * n);
                let mut tmp = Lexer::new(window, true);
                let mut num_args = 0usize;
                loop {
                    let next = match tmp.get_obj() {
                        Ok(t) => t,
                        // A FormatError here escapes in pdf.js too, out of
                        // the whole parse; treat it as "not a command".
                        Err(_) => {
                            state = 0;
                            break;
                        }
                    };
                    match next {
                        Tok::Eof => {
                            state = 0;
                            break;
                        }
                        Tok::Obj(Obj::Cmd(c)) => {
                            match OP_MAP.iter().find(|(k, _)| *k == &*c) {
                                Some((
                                    _,
                                    Some(OpSpec {
                                        num_args: na,
                                        variable_args,
                                        ..
                                    }),
                                )) => {
                                    let ok = if *variable_args {
                                        num_args <= *na
                                    } else {
                                        num_args == *na
                                    };
                                    if ok {
                                        break;
                                    }
                                }
                                _ => {
                                    state = 0;
                                    break;
                                }
                            }
                            num_args = 0;
                            continue;
                        }
                        Tok::Obj(_) => num_args += 1,
                    }
                }
                if state == 2 {
                    break;
                }
            } else {
                state = 0;
            }
        }
        if ch == -1
            && let Some(p) = maybe_ei_pos
        {
            let back = self.lexer.stream.pos - p;
            self.lexer.stream.skip(-(back as isize));
        }
        let mut end_offset = 4isize;
        self.lexer.stream.skip(-end_offset);
        let c = self.lexer.stream.peek_byte();
        self.lexer.stream.skip(end_offset);
        if !is_white_space(c) {
            end_offset -= 1;
        }
        (self.lexer.stream.pos as isize - end_offset - start_pos as isize).max(0) as usize
    }

    fn find_dct_decode_inline_stream_end(&mut self) -> usize {
        let start_pos = self.lexer.stream.pos;
        let mut found_eoi = false;
        let mut b;
        loop {
            b = self.lexer.stream.get_byte();
            if b == -1 {
                break;
            }
            if b != 0xff {
                continue;
            }
            match self.lexer.stream.get_byte() {
                0x00 => {}
                0xff => self.lexer.stream.skip(-1),
                0xd9 => found_eoi = true,
                0xc0..=0xc3
                | 0xc5..=0xc7
                | 0xc9..=0xcb
                | 0xcd..=0xcf
                | 0xc4
                | 0xcc
                | 0xda
                | 0xdb
                | 0xdc
                | 0xdd
                | 0xde
                | 0xdf
                | 0xe0..=0xef
                | 0xfe => {
                    let marker_length = self.lexer.stream.get_uint16();
                    if marker_length > 2 {
                        self.lexer.stream.skip(marker_length as isize - 2);
                    } else {
                        self.lexer.stream.skip(-2);
                    }
                }
                _ => {}
            }
            if found_eoi {
                break;
            }
        }
        let length = self.lexer.stream.pos - start_pos;
        if b == -1 {
            self.lexer.stream.skip(-(length as isize));
            return self.find_default_inline_stream_end();
        }
        self.inline_stream_skip_ei();
        length
    }

    fn find_ascii85_decode_inline_stream_end(&mut self) -> usize {
        let start_pos = self.lexer.stream.pos;
        let mut ch;
        loop {
            ch = self.lexer.stream.get_byte();
            if ch == -1 {
                break;
            }
            if ch == 0x7e {
                let tilde_pos = self.lexer.stream.pos;
                ch = self.lexer.stream.peek_byte();
                while is_white_space(ch) {
                    self.lexer.stream.skip(1);
                    ch = self.lexer.stream.peek_byte();
                }
                if ch == 0x3e {
                    self.lexer.stream.skip(1);
                    break;
                }
                if self.lexer.stream.pos > tilde_pos {
                    let maybe = self.lexer.stream.peek_bytes(2);
                    if maybe.len() == 2 && maybe[0] == 0x45 && maybe[1] == 0x49 {
                        break;
                    }
                }
            }
        }
        let length = self.lexer.stream.pos - start_pos;
        if ch == -1 {
            self.lexer.stream.skip(-(length as isize));
            return self.find_default_inline_stream_end();
        }
        self.inline_stream_skip_ei();
        length
    }

    fn find_ascii_hex_decode_inline_stream_end(&mut self) -> usize {
        let start_pos = self.lexer.stream.pos;
        let mut ch;
        loop {
            ch = self.lexer.stream.get_byte();
            if ch == -1 || ch == 0x3e {
                break;
            }
        }
        let length = self.lexer.stream.pos - start_pos;
        if ch == -1 {
            self.lexer.stream.skip(-(length as isize));
            return self.find_default_inline_stream_end();
        }
        self.inline_stream_skip_ei();
        length
    }

    fn inline_stream_skip_ei(&mut self) {
        let mut state = 0;
        loop {
            let ch = self.lexer.stream.get_byte();
            if ch == -1 {
                break;
            }
            if state == 0 {
                state = if ch == 0x45 { 1 } else { 0 };
            } else if state == 1 {
                state = if ch == 0x49 { 2 } else { 0 };
            } else if state == 2 {
                break;
            }
        }
    }

    fn make_inline_image(&mut self) -> Result<Rc<Stream>, FormatError> {
        let mut dict = Dict::default();
        while !self.buf1.is_cmd("ID") && !matches!(self.buf1, Tok::Eof) {
            let key = match &self.buf1 {
                Tok::Obj(Obj::Name(n)) => n.clone(),
                _ => return Err(FormatError),
            };
            self.shift()?;
            if matches!(self.buf1, Tok::Eof) {
                break;
            }
            if let Tok::Obj(o) = self.get_obj()? {
                dict.set(&key, o);
            }
        }
        let mut dict_length: isize = 0;
        if self.lexer.begin_inline_image_pos != -1 {
            dict_length = self.lexer.stream.pos as isize - self.lexer.begin_inline_image_pos;
        }
        let filter = dict.get_raw("F").or_else(|| dict.get_raw("Filter"));
        let filter_name: Option<String> = match filter {
            Some(Obj::Name(n)) => Some(n.to_string()),
            Some(Obj::Array(a)) => a.first().and_then(|f| f.name()).map(str::to_string),
            _ => None,
        };
        let start_pos = self.lexer.stream.pos;
        let length = match filter_name.as_deref() {
            Some("DCT") | Some("DCTDecode") => self.find_dct_decode_inline_stream_end(),
            Some("A85") | Some("ASCII85Decode") => self.find_ascii85_decode_inline_stream_end(),
            Some("AHx") | Some("ASCIIHexDecode") => self.find_ascii_hex_decode_inline_stream_end(),
            _ => self.find_default_inline_stream_end(),
        };
        let data = self.lexer.stream.data;
        let end = (start_pos + length).min(data.len());
        let bytes = data[start_pos.min(end)..end].to_vec();
        let mut cache_key: Option<Vec<u8>> = None;
        if length < 1000 && dict_length > 0 {
            let b = self.lexer.begin_inline_image_pos as usize;
            let e = (b + dict_length as usize + length).min(data.len());
            let mut key = data[b.min(e)..e].to_vec();
            key.extend_from_slice(&(e - b).to_le_bytes());
            if let Some((_, s)) = self.image_cache.iter().find(|(k, _)| *k == key) {
                let s = s.clone();
                self.buf2 = cmd("EI");
                self.shift()?;
                return Ok(s);
            }
            cache_key = Some(key);
        }
        let stream = Rc::new(Stream {
            dict: Rc::new(dict),
            source: StreamSource::Inline(bytes),
        });
        if let Some(key) = cache_key {
            self.image_id += 1;
            self.inline_keys
                .push((Rc::as_ptr(&stream), format!("inline_img_{}", self.image_id)));
            self.image_cache.push((key, stream.clone()));
        }
        self.buf2 = cmd("EI");
        self.shift()?;
        Ok(stream)
    }
}
