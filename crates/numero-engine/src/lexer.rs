//! Splits one line into tokens with byte spans: numbers, clock times,
//! symbols, known phrases (via the vocabulary trie), variables, and the
//! free text in between, which the parser later ignores.
//!
//! Numbers follow Numi's extractor (§2.1, §4.1): digits may be grouped
//! with spaces, `_` and the grouping separator (`1 234`, `1_234`,
//! `1,234.5`), and a number cannot end in a separator. A second decimal
//! point ends the number (`1.5.5` is `1.5` then `.5`). `0x`, `0b`, `0o`
//! and `1e3` literals remember their numeral system, so they print back
//! the same way.
//!
//! Variables match case-insensitively and may contain spaces (`total
//! cost`); the longest assigned name wins, but operator words (`in`, `of`)
//! still beat a variable of the same name (§5).

use std::ops::Range;

use crate::num::Num;
use crate::trie::is_word_char;
use crate::vocab::{Meaning, NumFmt, Vocab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sym {
    Plus,
    Minus,
    Times,
    Divide,
    Pow,
    Amp,
    Pipe,
    Shl,
    Shr,
    Assign,
    Square,
    Cube,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(Num, NumFmt),
    /// A clock time such as `14:30` or `2:30:15`; `padded` when the hour
    /// was written with a leading zero (`09:30`), which rules out PM.
    Time {
        hour: u32,
        minute: u32,
        second: u32,
        padded: bool,
    },
    /// `am` (false) or `pm` (true) after a time or an hour.
    Meridiem(bool),
    Sym(Sym),
    LParen,
    RParen,
    /// Argument separator `;`, or a `,` that is not inside a number.
    Sep,
    Phrase(Vec<Meaning>),
    Var(String),
    /// Free text: ignored by the parser.
    Ident,
    /// `// comment` or `"quoted text"`.
    Comment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Range<usize>,
}

/// How numbers are written: which character is the decimal point.
#[derive(Debug, Clone, Copy)]
pub struct NumberStyle {
    pub decimal: char,
}

/// Tokenize `line[start..]`; spans index into `line`. `vars` holds the
/// assigned variable names, lower-cased with single spaces.
pub fn lex(
    line: &str,
    start: usize,
    vocab: &Vocab,
    style: NumberStyle,
    vars: &[String],
) -> Vec<Token> {
    let mut lexer = Lexer {
        line,
        i: start,
        tokens: Vec::new(),
        vocab,
        style,
        vars,
    };
    lexer.run();
    lexer.tokens
}

struct Lexer<'a> {
    line: &'a str,
    i: usize,
    tokens: Vec<Token>,
    vocab: &'a Vocab,
    style: NumberStyle,
    vars: &'a [String],
}

impl Lexer<'_> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.i += c.len_utf8();
            } else if self.rest().starts_with("//") {
                self.push(Tok::Comment, self.line.len());
            } else if c == '"' {
                let end = self.rest()[1..]
                    .find('"')
                    .map_or(self.line.len(), |p| self.i + p + 2);
                self.push(Tok::Comment, end);
            } else if !self.number()
                && !self.meridiem()
                && !self.variable()
                && !self.times_x()
                && !self.gmt_offset()
            {
                self.word_or_symbol(c);
            }
        }
    }

    fn rest(&self) -> &str {
        &self.line[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn push(&mut self, tok: Tok, end: usize) {
        self.tokens.push(Token {
            tok,
            span: self.i..end,
        });
        self.i = end;
    }

    fn prev(&self) -> Option<&Tok> {
        self.tokens.last().map(|t| &t.tok)
    }

    fn at_word_boundary(&self) -> bool {
        let prev_char = self.line[..self.i].chars().next_back();
        prev_char.is_none_or(|p| !is_word_char(p) || p.is_ascii_digit())
    }

    fn number(&mut self) -> bool {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let starts_number = bytes.first().is_some_and(u8::is_ascii_digit)
            || (bytes.first() == Some(&b'.') && bytes.get(1).is_some_and(u8::is_ascii_digit));
        if !starts_number {
            return false;
        }
        if let Some((tok, len)) = clock_time(rest) {
            self.push(tok, self.i + len);
        } else if let Some((num, fmt, len)) = radix_literal(rest) {
            self.push(Tok::Num(num, fmt), self.i + len);
        } else {
            let (text, len, scientific) = decimal_literal(rest, self.style.decimal);
            let fmt = if scientific {
                NumFmt::Scientific
            } else {
                NumFmt::Decimal
            };
            match Num::parse(&text) {
                Some(num) => self.push(Tok::Num(num, fmt), self.i + len),
                None => self.push(Tok::Ident, self.i + len),
            }
        }
        true
    }

    fn meridiem(&mut self) -> bool {
        let after_time = match self.prev() {
            Some(Tok::Time { .. }) => true,
            Some(Tok::Num(n, _)) => n.to_i64().is_some_and(|h| (1..=12).contains(&h)),
            _ => false,
        };
        if !after_time {
            return false;
        }
        let lower: String = self
            .rest()
            .chars()
            .take(4)
            .collect::<String>()
            .to_lowercase();
        let (pm, len) = if lower.starts_with("a.m.") || lower.starts_with("p.m.") {
            (lower.starts_with('p'), 4)
        } else if (lower.starts_with("am") || lower.starts_with("pm"))
            && !lower.chars().nth(2).is_some_and(is_word_char)
        {
            (lower.starts_with('p'), 2)
        } else {
            return false;
        };
        self.push(Tok::Meridiem(pm), self.i + len);
        true
    }

    /// `x` between two numbers multiplies: `3 x 4`, `3x4`, `2 m x 3`; so
    /// does an `x` glued to the number after it (`x2`, which then has no
    /// left operand).
    fn times_x(&mut self) -> bool {
        let after = self.rest().get(1..).unwrap_or("");
        let glued = after.chars().next().is_some_and(|c| c.is_ascii_digit());
        let after_value = match self.prev() {
            Some(Tok::Num(..) | Tok::RParen) => true,
            Some(Tok::Phrase(ms)) => ms
                .iter()
                .any(|m| matches!(m, Meaning::Unit(_) | Meaning::Currency(_))),
            _ => false,
        };
        if self.peek() != Some('x') || !(after_value || glued && self.at_word_boundary()) {
            return false;
        }
        let next = after.trim_start().chars().next();
        let alone = !after.chars().next().is_some_and(is_word_char);
        if (glued || alone) && next.is_some_and(|c| c.is_ascii_digit() || c == '(' || c == '.') {
            self.push(Tok::Sym(Sym::Times), self.i + 1);
            return true;
        }
        false
    }

    /// The longest assigned variable name at this position, unless an
    /// operator word of at least the same length starts here (`in = 4`
    /// assigns, but `in * 2` still reads `in` as the conversion word).
    fn variable(&mut self) -> bool {
        if !self.peek().is_some_and(|c| c.is_alphabetic() || c == '_') || !self.at_word_boundary() {
            return false;
        }
        let Some(len) = self
            .vars
            .iter()
            .filter_map(|name| match_name(self.rest(), name))
            .max()
        else {
            return false;
        };
        let end = self.i + len;
        let operator = self
            .vocab
            .trie
            .longest_match(self.line, self.i)
            .is_some_and(|m| m.end >= end && m.meanings.iter().copied().any(is_operator_word));
        if operator {
            return false;
        }
        let name = normalize_name(&self.line[self.i..end]);
        self.push(Tok::Var(name), end);
        true
    }

    /// `GMT+3`, `gmt -5`, `GMT+5:30` as one token (see
    /// [`Meaning::GmtOffset`]). `UTC+3` is the zone `UTC` and then `+ 3`.
    fn gmt_offset(&mut self) -> bool {
        let rest = self.rest();
        if !rest.get(..3).is_some_and(|h| h.eq_ignore_ascii_case("gmt")) || !self.at_word_boundary()
        {
            return false;
        }
        let after = rest[3..].trim_start();
        let Some(sign) = after
            .chars()
            .next()
            .filter(|c| matches!(c, '+' | '-' | '−'))
        else {
            return false;
        };
        let digits_part = after[sign.len_utf8()..].trim_start();
        let digits = digits_part
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b':')
            .count();
        if digits == 0
            || digits_part[digits..]
                .chars()
                .next()
                .is_some_and(is_word_char)
        {
            return false;
        }
        let len = rest.len() - digits_part.len() + digits;
        self.push(Tok::Phrase(vec![Meaning::GmtOffset]), self.i + len);
        true
    }

    fn word_or_symbol(&mut self, c: char) {
        let word_len = self
            .rest()
            .find(|ch: char| !is_word_char(ch))
            .unwrap_or(self.rest().len());
        let at_boundary = self.at_word_boundary();
        if (at_boundary || !is_word_char(c))
            && let Some(m) = self.vocab.trie.longest_match(self.line, self.i)
        {
            self.push(Tok::Phrase(m.meanings), m.end);
            return;
        }
        if at_boundary && let Some((meanings, len)) = self.function_before_digit(word_len) {
            self.push(Tok::Phrase(meanings), self.i + len);
            return;
        }
        if let Some((tok, len)) = symbol(self.rest()) {
            self.push(tok, self.i + len);
            return;
        }
        let len = if word_len > 0 { word_len } else { c.len_utf8() };
        self.push(Tok::Ident, self.i + len);
    }
}

impl Lexer<'_> {
    /// `log2(8)`: a function name glued to the number after it.
    fn function_before_digit(&self, word_len: usize) -> Option<(Vec<Meaning>, usize)> {
        let rest = self.rest();
        let letters = rest.find(|c: char| !c.is_alphabetic())?;
        if letters == 0
            || letters >= word_len
            || !rest[letters..].starts_with(|c: char| c.is_ascii_digit())
        {
            return None;
        }
        let head = &self.line[..self.i + letters];
        let m = self.vocab.trie.longest_match(head, self.i)?;
        let functions: Vec<Meaning> = m
            .meanings
            .into_iter()
            .filter(|m| matches!(m, Meaning::Func(_)))
            .collect();
        (m.end == head.len() && !functions.is_empty()).then_some((functions, letters))
    }
}

/// Operator and keyword meanings that beat a variable of the same name.
fn is_operator_word(meaning: Meaning) -> bool {
    matches!(
        meaning,
        Meaning::Conversion
            | Meaning::Assign
            | Meaning::Plus
            | Meaning::Minus
            | Meaning::Times
            | Meaning::Divide
            | Meaning::Mod
            | Meaning::Xor
            | Meaning::Pct(_)
            | Meaning::Percent
    )
}

/// A variable name as stored: lower case, whitespace runs folded to one
/// space.
pub fn normalize_name(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Byte length of `rest`'s prefix that spells `name` (lower case, single
/// spaces) ignoring case, ending on a word boundary.
fn match_name(rest: &str, name: &str) -> Option<usize> {
    let mut text = rest.char_indices().peekable();
    let mut end = 0;
    for expected in name.chars() {
        let (i, c) = text.next()?;
        if expected == ' ' {
            if !c.is_whitespace() {
                return None;
            }
            while text.peek().is_some_and(|(_, w)| w.is_whitespace()) {
                text.next();
            }
        } else if !c.to_lowercase().eq(std::iter::once(expected)) {
            return None;
        }
        end = i + c.len_utf8();
    }
    let boundary = text.peek().is_none_or(|(_, c)| !is_word_char(*c));
    (end > 0 && boundary).then_some(end)
}

fn symbol(rest: &str) -> Option<(Tok, usize)> {
    const TWO: &[(&str, Tok)] = &[
        ("**", Tok::Sym(Sym::Pow)),
        ("<<", Tok::Sym(Sym::Shl)),
        (">>", Tok::Sym(Sym::Shr)),
    ];
    for (text, tok) in TWO {
        if rest.starts_with(text) {
            return Some((tok.clone(), 2));
        }
    }
    let c = rest.chars().next()?;
    let tok = match c {
        '+' => Tok::Sym(Sym::Plus),
        '-' | '‒' | '–' | '—' | '―' | '−' => Tok::Sym(Sym::Minus),
        '*' | '×' | '✕' | '·' => Tok::Sym(Sym::Times),
        '/' | '÷' => Tok::Sym(Sym::Divide),
        '^' | 'ˆ' => Tok::Sym(Sym::Pow),
        '&' => Tok::Sym(Sym::Amp),
        '|' => Tok::Sym(Sym::Pipe),
        '=' => Tok::Sym(Sym::Assign),
        '²' => Tok::Sym(Sym::Square),
        '³' => Tok::Sym(Sym::Cube),
        '(' => Tok::LParen,
        ')' => Tok::RParen,
        ',' | ';' => Tok::Sep,
        _ => return None,
    };
    Some((tok, c.len_utf8()))
}

/// `14:30`, `2:30`, `23:59:59`.
fn clock_time(rest: &str) -> Option<(Tok, usize)> {
    let bytes = rest.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let h_len = digits(0);
    if !(1..=2).contains(&h_len) || bytes.get(h_len) != Some(&b':') || digits(h_len + 1) != 2 {
        return None;
    }
    let hour: u32 = rest[..h_len].parse().ok()?;
    let minute: u32 = rest[h_len + 1..h_len + 3].parse().ok()?;
    let mut len = h_len + 3;
    let mut second = 0;
    if bytes.get(len) == Some(&b':') && digits(len + 1) == 2 {
        second = rest[len + 1..len + 3].parse().ok()?;
        len += 3;
    }
    (hour < 24 && minute < 60 && second < 60).then_some((
        Tok::Time {
            hour,
            minute,
            second,
            padded: h_len == 2 && bytes[0] == b'0',
        },
        len,
    ))
}

/// `0xFF`, `0b1010`, `0o17`.
fn radix_literal(rest: &str) -> Option<(Num, NumFmt, usize)> {
    let (radix, fmt) = match rest.get(..2)? {
        "0x" | "0X" => (16, NumFmt::Hex),
        "0b" | "0B" => (2, NumFmt::Binary),
        "0o" | "0O" => (8, NumFmt::Octal),
        _ => return None,
    };
    let digits = rest[2..].chars().take_while(|c| c.is_digit(radix)).count();
    if digits == 0 {
        return None;
    }
    let value = i128::from_str_radix(&rest[2..2 + digits], radix).ok()?;
    Some((Num::from_i128(value), fmt, 2 + digits))
}

/// A decimal literal: the longest run of digits and separators that ends
/// in a digit, then an optional exponent. Returns the literal in
/// `1234.5` form, its length in the line, and whether it had an exponent.
fn decimal_literal(rest: &str, decimal: char) -> (String, usize, bool) {
    let is_separator = |c: char| c == ' ' || c == '_' || c == ',' || c == '.' || c == '\u{a0}';
    let mut end = 0;
    let mut points = 0;
    for (i, c) in rest.char_indices() {
        if c.is_ascii_digit() {
            end = i + 1;
        } else if c == decimal {
            points += 1;
            if points > 1 {
                break;
            }
        } else if !is_separator(c) {
            break;
        }
    }
    let mut text = String::with_capacity(end);
    let mut seen_point = false;
    for c in rest[..end].chars() {
        if c.is_ascii_digit() {
            text.push(c);
        } else if c == decimal && !seen_point {
            text.push('.');
            seen_point = true;
        }
    }
    let bytes = rest.as_bytes();
    let mut scientific = false;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exp = bytes[end + 1 + sign..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if exp > 0
            && !bytes
                .get(end + 1 + sign + exp)
                .is_some_and(|b| b.is_ascii_alphabetic())
        {
            text.push_str(&rest[end..end + 1 + sign + exp]);
            end += 1 + sign + exp;
            scientific = true;
        }
    }
    (text, end, scientific)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::english;

    const EN: NumberStyle = NumberStyle { decimal: '.' };

    fn kinds(line: &str) -> Vec<Tok> {
        lex(line, 0, &english(), EN, &[])
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    fn num(text: &str) -> Tok {
        Tok::Num(Num::exact(text), NumFmt::Decimal)
    }

    #[test]
    fn numbers_group_with_spaces_underscores_and_commas() {
        assert_eq!(kinds("1,000.5"), vec![num("1000.5")]);
        assert_eq!(kinds("1 234"), vec![num("1234")]);
        assert_eq!(kinds("1_234"), vec![num("1234")]);
        assert_eq!(kinds("5, "), vec![num("5"), Tok::Sep]);
    }

    #[test]
    fn literals_keep_their_numeral_system() {
        assert_eq!(
            kinds("1e3"),
            vec![Tok::Num(Num::exact("1000"), NumFmt::Scientific)]
        );
        assert_eq!(kinds("0xFF"), vec![Tok::Num(Num::int(255), NumFmt::Hex)]);
    }

    #[test]
    fn x_multiplies_between_numbers_only() {
        assert_eq!(kinds("3x4")[1], Tok::Sym(Sym::Times));
        assert_eq!(kinds("3 x 4")[1], Tok::Sym(Sym::Times));
        assert_eq!(kinds("x 4")[0], Tok::Ident);
    }

    #[test]
    fn clock_times_and_meridiem() {
        assert_eq!(kinds("2:30 pm")[1], Tok::Meridiem(true));
        assert_eq!(kinds("3pm")[1], Tok::Meridiem(true));
        assert!(matches!(kinds("09:30")[0], Tok::Time { padded: true, .. }));
        assert!(matches!(kinds("9:30")[0], Tok::Time { padded: false, .. }));
        // `GMT+3` is one token; `UTC+3` is the zone and `+ 3` (§8.2 [oracle]).
        assert_eq!(kinds("GMT+3"), vec![Tok::Phrase(vec![Meaning::GmtOffset])]);
        assert_eq!(kinds("gmt -5"), vec![Tok::Phrase(vec![Meaning::GmtOffset])]);
        assert_eq!(kinds("UTC+3")[1], Tok::Sym(Sym::Plus));
    }

    #[test]
    fn a_second_decimal_point_ends_the_number() {
        assert_eq!(kinds("1.5.5"), vec![num("1.5"), num(".5")]);
    }

    #[test]
    fn variables_ignore_case_and_may_hold_spaces() {
        let vars = ["total cost".to_string(), "in".to_string()];
        let toks = |line: &str| -> Vec<Tok> {
            lex(line, 0, &english(), EN, &vars)
                .into_iter()
                .map(|t| t.tok)
                .collect()
        };
        assert_eq!(toks("Total  Cost * 2")[0], Tok::Var("total cost".into()));
        assert!(matches!(&toks("in * 2")[0], Tok::Phrase(_)));
        assert_eq!(toks("total costs")[0], Tok::Phrase(vec![Meaning::Sum]));
    }

    #[test]
    fn function_names_glue_to_digits() {
        assert!(matches!(&kinds("log2(8)")[0], Tok::Phrase(m) if matches!(m[0], Meaning::Func(_))));
        assert_eq!(kinds("log2(8)")[1], num("2"));
    }

    #[test]
    fn spans_are_ordered_and_disjoint() {
        let line = "$9 for coffee + 5 km in miles // note";
        let tokens = lex(line, 0, &english(), EN, &[]);
        for pair in tokens.windows(2) {
            assert!(pair[0].span.end <= pair[1].span.start);
        }
    }
}
