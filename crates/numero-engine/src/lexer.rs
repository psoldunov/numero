//! Splits one line into tokens with byte spans: numbers, clock times,
//! symbols, known phrases (via the vocabulary trie), variables, and the
//! free text in between, which the parser later ignores.
//!
//! Numbers follow Numi's extractor (§2.1, §4.1): digits may be grouped
//! with spaces, `_` and the grouping separator (`1 234`, `1_234`,
//! `1,234.5`), and a number cannot end in a separator. `0x`, `0b`, `0o`
//! and `1e3` literals remember their numeral system, so they print back
//! the same way.

use std::ops::Range;

use crate::num::Num;
use crate::trie::is_word_char;
use crate::vocab::{Meaning, NumFmt, Vocab};
use crate::zones;

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
    /// A clock time such as `14:30` or `2:30:15`.
    Time {
        hour: u32,
        minute: u32,
        second: u32,
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

/// Tokenize `line[start..]`; spans index into `line`.
pub fn lex(
    line: &str,
    start: usize,
    vocab: &Vocab,
    style: NumberStyle,
    is_var: &dyn Fn(&str) -> bool,
) -> Vec<Token> {
    let mut lexer = Lexer {
        line,
        i: start,
        tokens: Vec::new(),
        vocab,
        style,
        is_var,
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
    is_var: &'a dyn Fn(&str) -> bool,
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
            } else if !self.number() && !self.meridiem() && !self.times_x() && !self.gmt_offset() {
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

    /// `x` between two numbers multiplies: `3 x 4`, `3x4`.
    fn times_x(&mut self) -> bool {
        if self.peek() != Some('x') || !matches!(self.prev(), Some(Tok::Num(..) | Tok::RParen)) {
            return false;
        }
        let after = &self.rest()[1..];
        let next = after.trim_start().chars().next();
        let glued = after.chars().next().is_some_and(|c| c.is_ascii_digit());
        let alone = !after.chars().next().is_some_and(is_word_char);
        if (glued || alone) && next.is_some_and(|c| c.is_ascii_digit() || c == '(' || c == '.') {
            self.push(Tok::Sym(Sym::Times), self.i + 1);
            return true;
        }
        false
    }

    /// `GMT+3`, `utc -5`, `gmt+5:30`… as a fixed-offset zone (§8.2).
    fn gmt_offset(&mut self) -> bool {
        let rest = self.rest();
        let Some(head) = rest.get(..3) else {
            return false;
        };
        if !(head.eq_ignore_ascii_case("gmt") || head.eq_ignore_ascii_case("utc"))
            || !self.at_word_boundary()
        {
            return false;
        }
        let after = &rest[3..];
        let signed = after.trim_start();
        let Some(sign) = signed
            .chars()
            .next()
            .filter(|c| matches!(c, '+' | '-' | '−'))
        else {
            return false;
        };
        let digits_part = signed[sign.len_utf8()..].trim_start();
        let digits = digits_part.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=2).contains(&digits)
            || digits_part[digits..]
                .chars()
                .next()
                .is_some_and(is_word_char)
        {
            return false;
        }
        let hours: i64 = digits_part[..digits].parse().unwrap_or(0);
        let hours = if sign == '+' { hours } else { -hours };
        let Some(tz) = zones::fixed_offset(hours) else {
            return false;
        };
        let len = rest.len() - digits_part.len() + digits;
        self.push(Tok::Phrase(vec![Meaning::Zone(tz)]), self.i + len);
        true
    }

    fn word_or_symbol(&mut self, c: char) {
        let word_len = self
            .rest()
            .find(|ch: char| !is_word_char(ch))
            .unwrap_or(self.rest().len());
        let at_boundary = self.at_word_boundary();
        if word_len > 0 && at_boundary && (self.is_var)(&self.rest()[..word_len]) {
            let name = self.rest()[..word_len].to_string();
            self.push(Tok::Var(name), self.i + word_len);
            return;
        }
        if (at_boundary || !is_word_char(c))
            && let Some(m) = self.vocab.trie.longest_match(self.line, self.i)
        {
            self.push(Tok::Phrase(m.meanings), m.end);
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
    for (i, c) in rest.char_indices() {
        if c.is_ascii_digit() {
            end = i + 1;
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
        lex(line, 0, &english(), EN, &|_| false)
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
    fn clock_times_meridiem_and_offsets() {
        assert_eq!(kinds("2:30 pm")[1], Tok::Meridiem(true));
        assert_eq!(kinds("3pm")[1], Tok::Meridiem(true));
        assert!(matches!(&kinds("GMT+3")[0], Tok::Phrase(m) if matches!(m[0], Meaning::Zone(_))));
        assert!(matches!(&kinds("gmt -5")[0], Tok::Phrase(m) if matches!(m[0], Meaning::Zone(_))));
    }

    #[test]
    fn spans_are_ordered_and_disjoint() {
        let line = "$9 for coffee + 5 km in miles // note";
        let tokens = lex(line, 0, &english(), EN, &|_| false);
        for pair in tokens.windows(2) {
            assert!(pair[0].span.end <= pair[1].span.start);
        }
    }
}
