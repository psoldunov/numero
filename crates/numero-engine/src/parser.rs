//! From tokens to an expression tree, in two passes.
//!
//! 1. [`resolve`] picks one meaning for every phrase from its neighbours and
//!    drops what does not take part in the calculation: free text, a unit
//!    word with no number next to it, `and` with nothing to join. That is
//!    how `$9 for coffee + $3 tip` reduces to `$9 + $3`. A few things make
//!    the whole line fail instead, as in Numi: a conversion word followed
//!    by an unknown word (`5 m in foo`), a function name with no argument
//!    after it (`sqrt of 16`), a numeral-system word before a value
//!    (`exp(2)`).
//! 2. [`parse`] runs a Pratt parser over what is left with Numi's
//!    priorities (§2.2): `+ −` and the percent phrases at 100; `× ÷ mod &
//!    | xor << >>` at 200; `^` and `in` at 300; all left-associative.
//!    Unary minus binds tightest, then postfix units, `%`, scales, `²`.
//!
//! Numi tolerates loose input, and so does the parser (§2.3 [oracle]): a
//! value followed directly by a parenthesised group multiplies (`2(3)`,
//! `x(2)`); a number with a unit after another one adds (`1 h 30 min`); any
//! other value that follows a value is dropped, so the first one wins
//! (`2 pi` = 2, `10 % 3 + 1` = 11 %); an operator with nothing on one side
//! is dropped (`* 2`, `5 +`). There is no `!`.

use chrono_tz::Tz;

use crate::TokenClass;
use crate::lexer::{Sym, Tok, Token};
use crate::num::Num;
use crate::units::UnitRef;
use crate::vocab::{Func, Meaning, NumFmt, PctOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateWord {
    Today,
    Tomorrow,
    Yesterday,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Unit(UnitRef),
    Currency(u16),
    Format(NumFmt),
    Zone(Tz),
    Local,
    Percent,
    /// A month name after `in`: Numi accepts it and the line fails.
    Month,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(Num, NumFmt),
    Var(String),
    Pi,
    E,
    Prev,
    Sum,
    Avg,
    Date(DateWord),
    Now(Option<Tz>),
    Clock {
        hour: u32,
        minute: u32,
        second: u32,
        padded: bool,
    },
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Pct(PctOp, Box<Expr>, Box<Expr>),
    Percent(Box<Expr>),
    Unit(Box<Expr>, UnitRef),
    Currency(Box<Expr>, u16),
    Scale(Box<Expr>, Num),
    Meridiem(Box<Expr>, bool),
    InZone(Box<Expr>, Tz),
    /// `5 oct`: a day of the given month.
    Day(Box<Expr>, u8),
    Call(Func, Vec<Expr>),
    Convert(Box<Expr>, Target),
}

/// A token with exactly one meaning, ready for the Pratt parser.
#[derive(Debug, Clone, PartialEq)]
pub enum P {
    Num(Num, NumFmt),
    Clock {
        hour: u32,
        minute: u32,
        second: u32,
        padded: bool,
    },
    Meridiem(bool),
    Var(String),
    Op(BinOp),
    Square,
    Cube,
    LParen,
    RParen,
    Sep,
    Unit(UnitRef),
    /// A unit before its number: `km 5`, `at 5`.
    UnitPrefix(UnitRef),
    CurrencyPrefix(u16),
    CurrencySuffix(u16),
    Scale(Num),
    Percent,
    Pct(PctOp),
    Conv,
    Target(Target),
    Func(Func),
    Pi,
    E,
    Sum,
    Avg,
    Prev,
    Date(DateWord),
    Now,
    Zone(Tz),
    MonthPrefix(u8),
    MonthSuffix(u8),
    /// Something that makes the whole line fail. `resets` when Numi still
    /// gives the line an empty value, which starts the grand total over
    /// (`5 m in foo`), rather than skipping it (`sqrt of 16`) (§5 [oracle]).
    Fail {
        resets: bool,
    },
}

impl P {
    fn ends_value(&self) -> bool {
        matches!(
            self,
            P::Num(..)
                | P::Clock { .. }
                | P::Meridiem(_)
                | P::Var(_)
                | P::RParen
                | P::Square
                | P::Cube
                | P::Unit(_)
                | P::CurrencySuffix(_)
                | P::Scale(_)
                | P::Percent
                | P::Target(_)
                | P::Pi
                | P::E
                | P::Sum
                | P::Avg
                | P::Prev
                | P::Date(_)
                | P::Now
                | P::Zone(_)
                | P::MonthSuffix(_)
        )
    }

    fn is_time_like(&self) -> bool {
        matches!(self, P::Clock { .. } | P::Meridiem(_) | P::Now | P::Date(_))
    }

    /// Tokens that begin an operand of their own.
    fn starts_operand(&self) -> bool {
        matches!(
            self,
            P::Num(..)
                | P::Clock { .. }
                | P::Var(_)
                | P::Pi
                | P::E
                | P::Sum
                | P::Avg
                | P::Prev
                | P::Date(_)
                | P::Now
                | P::CurrencyPrefix(_)
                | P::UnitPrefix(_)
                | P::MonthPrefix(_)
                | P::Func(_)
        )
    }
}

fn starts_value(tok: &Tok) -> bool {
    match tok {
        Tok::Num(..) | Tok::Time { .. } | Tok::LParen | Tok::Var(_) => true,
        Tok::Sym(s) => matches!(s, Sym::Minus | Sym::Plus),
        Tok::Phrase(ms) => ms.iter().any(|m| {
            matches!(
                m,
                Meaning::Currency(_)
                    | Meaning::Func(_)
                    | Meaning::Pi
                    | Meaning::E
                    | Meaning::Sum
                    | Meaning::Avg
                    | Meaning::Prev
                    | Meaning::Today
                    | Meaning::Tomorrow
                    | Meaning::Yesterday
                    | Meaning::Now
                    | Meaning::Zone(_)
            )
        }),
        _ => false,
    }
}

fn as_target(meaning: Meaning) -> Option<Target> {
    match meaning {
        Meaning::Unit(u) => Some(Target::Unit(u)),
        Meaning::Currency(c) => Some(Target::Currency(c)),
        Meaning::Format(f) => Some(Target::Format(f)),
        Meaning::Zone(z) => Some(Target::Zone(z)),
        Meaning::Local => Some(Target::Local),
        Meaning::Percent => Some(Target::Percent),
        Meaning::Month(_) => Some(Target::Month),
        Meaning::GmtOffset => "Etc/GMT".parse().ok().map(Target::Zone),
        _ => None,
    }
}

/// The neighbours a phrase is resolved against.
struct Context<'a> {
    prev: Option<&'a P>,
    /// The next token that is not free text or a comment.
    next: Option<&'a Tok>,
    /// The token directly after, free text included.
    adjacent: Option<&'a Tok>,
}

/// The tokens that take part in the calculation, and a highlight class for
/// every input token (same order as `tokens`).
pub fn resolve(tokens: &[Token]) -> (Vec<P>, Vec<TokenClass>) {
    let mut out: Vec<P> = Vec::with_capacity(tokens.len());
    let mut classes = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        let context = Context {
            prev: out.last(),
            next: tokens[i + 1..]
                .iter()
                .map(|t| &t.tok)
                .find(|t| !matches!(t, Tok::Ident | Tok::Comment)),
            adjacent: tokens.get(i + 1).map(|t| &t.tok),
        };
        let (p, class) = resolve_one(&token.tok, &context);
        classes.push(class);
        if let Some(p) = p {
            out.push(p);
        }
    }
    (out, classes)
}

fn resolve_one(tok: &Tok, context: &Context<'_>) -> (Option<P>, TokenClass) {
    use TokenClass as C;
    match tok {
        Tok::Num(n, fmt) => (Some(P::Num(*n, *fmt)), C::Number),
        Tok::Time {
            hour,
            minute,
            second,
            padded,
        } => (
            Some(P::Clock {
                hour: *hour,
                minute: *minute,
                second: *second,
                padded: *padded,
            }),
            C::Number,
        ),
        Tok::Meridiem(pm) => (Some(P::Meridiem(*pm)), C::Keyword),
        Tok::Var(name) => (Some(P::Var(name.clone())), C::Variable),
        Tok::LParen => (Some(P::LParen), C::Operator),
        Tok::RParen => (Some(P::RParen), C::Operator),
        Tok::Sep => (Some(P::Sep), C::Operator),
        Tok::Sym(sym) => (resolve_sym(*sym), C::Operator),
        Tok::Ident => (None, C::Text),
        Tok::Comment => (None, C::Comment),
        Tok::Phrase(meanings) => resolve_phrase(meanings, context),
    }
}

fn resolve_sym(sym: Sym) -> Option<P> {
    Some(match sym {
        Sym::Plus => P::Op(BinOp::Add),
        Sym::Minus => P::Op(BinOp::Sub),
        Sym::Times => P::Op(BinOp::Mul),
        Sym::Divide => P::Op(BinOp::Div),
        Sym::Pow => P::Op(BinOp::Pow),
        Sym::Amp => P::Op(BinOp::And),
        Sym::Pipe => P::Op(BinOp::Or),
        Sym::Shl => P::Op(BinOp::Shl),
        Sym::Shr => P::Op(BinOp::Shr),
        Sym::Square => P::Square,
        Sym::Cube => P::Cube,
        Sym::Assign => return None,
    })
}

fn resolve_phrase(meanings: &[Meaning], context: &Context<'_>) -> (Option<P>, TokenClass) {
    if matches!(context.prev, Some(P::Conv))
        && let Some(target) = meanings.iter().find_map(|m| as_target(*m))
    {
        return (Some(P::Target(target)), target_class(target));
    }
    let next_is_now = matches!(context.next, Some(Tok::Phrase(ms)) if ms.contains(&Meaning::Now));
    let after_time = context.prev.is_some_and(P::is_time_like);
    if (next_is_now || after_time)
        && let Some(Meaning::Zone(tz)) = meanings.iter().find(|m| matches!(m, Meaning::Zone(_)))
    {
        return (Some(P::Zone(*tz)), TokenClass::Keyword);
    }
    // A numeral-system word comes last: `5 dec` is the 5th of December.
    let (formats, others): (Vec<Meaning>, Vec<Meaning>) = meanings
        .iter()
        .copied()
        .filter(|m| !matches!(m, Meaning::Zone(_)))
        .partition(|m| matches!(m, Meaning::Format(_)));
    for meaning in others.into_iter().chain(formats) {
        if let Some(resolved) = resolve_meaning(meaning, context) {
            return resolved;
        }
    }
    (None, TokenClass::Text)
}

fn resolve_meaning(meaning: Meaning, context: &Context<'_>) -> Option<(Option<P>, TokenClass)> {
    use TokenClass as C;
    let keep = |p: P, class: C| Some((Some(p), class));
    let drop = |class: C| Some((None, class));
    let fail = |resets: bool, class: C| Some((Some(P::Fail { resets }), class));
    let prev = context.prev;
    let prev_value = prev.is_some_and(P::ends_value);
    let next_value = context.next.is_some_and(starts_value);
    let next_number = matches!(context.next, Some(Tok::Num(..) | Tok::Time { .. }));
    match meaning {
        // A unit right after another unit or `%` is ignored: `5 m kg` is
        // `5 m`, `25 °C` is `25 °`, `10 % m` is `10 %` (§1.2 [oracle]).
        Meaning::Unit(_) if matches!(prev, Some(P::Unit(_) | P::Target(_) | P::Percent)) => {
            drop(C::Unit)
        }
        Meaning::Unit(u) if prev_value => keep(P::Unit(u), C::Unit),
        Meaning::Unit(u) if next_number => keep(P::UnitPrefix(u), C::Unit),
        Meaning::Currency(c) if prev_value => keep(P::CurrencySuffix(c), C::Currency),
        Meaning::Currency(c) if next_value => keep(P::CurrencyPrefix(c), C::Currency),
        // `may 5` is a date but `5 may` is not (§8.1 [oracle]).
        Meaning::Month(m) if m != 5 && matches!(prev, Some(P::Num(..))) => {
            keep(P::MonthSuffix(m), C::Keyword)
        }
        Meaning::Month(m) if matches!(context.next, Some(Tok::Num(..))) => {
            keep(P::MonthPrefix(m), C::Keyword)
        }
        Meaning::Scale(n) if matches!(prev, Some(P::Num(..))) => keep(P::Scale(n), C::Number),
        Meaning::Percent if prev_value => keep(P::Percent, C::Operator),
        Meaning::Pct(op)
            if next_value
                && if op.needs_percent_left() {
                    matches!(prev, Some(P::Percent | P::Var(_) | P::RParen))
                } else {
                    prev_value
                } =>
        {
            keep(P::Pct(op), C::Keyword)
        }
        Meaning::Conversion if prev_value => {
            if next_is_target(context.next) {
                keep(P::Conv, C::Keyword)
            } else if matches!(context.adjacent, Some(Tok::Ident)) {
                fail(true, C::Keyword)
            } else {
                drop(C::Keyword)
            }
        }
        Meaning::GmtOffset if prev.is_some_and(P::is_time_like) => drop(C::Keyword),
        Meaning::GmtOffset => fail(false, C::Keyword),
        Meaning::Format(_) if prev_value => drop(C::Keyword),
        Meaning::Format(_) => fail(false, C::Keyword),
        Meaning::Plus if prev_value && next_value => keep(P::Op(BinOp::Add), C::Operator),
        Meaning::Minus if prev_value && next_value => keep(P::Op(BinOp::Sub), C::Operator),
        Meaning::Times if prev_value && next_value => keep(P::Op(BinOp::Mul), C::Operator),
        Meaning::Divide if prev_value && next_value => keep(P::Op(BinOp::Div), C::Operator),
        Meaning::Mod if prev_value && next_value => keep(P::Op(BinOp::Mod), C::Operator),
        Meaning::Xor if prev_value && next_value => keep(P::Op(BinOp::Xor), C::Operator),
        Meaning::Func(f) if next_value => keep(P::Func(f), C::Function),
        Meaning::Func(_) => fail(false, C::Function),
        Meaning::Pi => keep(P::Pi, C::Keyword),
        Meaning::E => keep(P::E, C::Keyword),
        Meaning::Sum => keep(P::Sum, C::Keyword),
        Meaning::Avg => keep(P::Avg, C::Keyword),
        Meaning::Prev => keep(P::Prev, C::Keyword),
        Meaning::Today => keep(P::Date(DateWord::Today), C::Keyword),
        Meaning::Tomorrow => keep(P::Date(DateWord::Tomorrow), C::Keyword),
        Meaning::Yesterday => keep(P::Date(DateWord::Yesterday), C::Keyword),
        Meaning::Now => keep(P::Now, C::Keyword),
        _ => None,
    }
}

fn next_is_target(next: Option<&Tok>) -> bool {
    matches!(next, Some(Tok::Phrase(ms)) if ms.iter().any(|m| as_target(*m).is_some()))
}

fn target_class(target: Target) -> TokenClass {
    match target {
        Target::Unit(_) => TokenClass::Unit,
        Target::Currency(_) => TokenClass::Currency,
        Target::Percent => TokenClass::Operator,
        _ => TokenClass::Keyword,
    }
}

/// Numi's priorities (§2.2-2.3) as binding powers: 100 → 10, 200 → 20,
/// 300 → 30. Ties go to the leftmost operator, so every operator is
/// left-associative, `^` included. Unary minus (priority 1000) binds
/// tightest: `-2^2` = 4.
const BP_ADD: (u8, u8) = (10, 11);
const BP_MUL: (u8, u8) = (20, 21);
const BP_POW: (u8, u8) = (30, 31);
const BP_CONVERT: u8 = 30;
const BP_UNARY: u8 = 40;
const BP_POSTFIX: u8 = 50;

fn infix_bp(p: &P) -> Option<(u8, u8)> {
    Some(match p {
        P::Op(BinOp::Add | BinOp::Sub) | P::Pct(_) => BP_ADD,
        P::Op(
            BinOp::Mul
            | BinOp::Div
            | BinOp::Mod
            | BinOp::And
            | BinOp::Or
            | BinOp::Xor
            | BinOp::Shl
            | BinOp::Shr,
        ) => BP_MUL,
        P::Op(BinOp::Pow) => BP_POW,
        _ => return None,
    })
}

fn is_postfix(p: &P) -> bool {
    matches!(
        p,
        P::Square
            | P::Cube
            | P::Unit(_)
            | P::CurrencySuffix(_)
            | P::Scale(_)
            | P::Percent
            | P::Meridiem(_)
            | P::Zone(_)
            | P::MonthSuffix(_)
    )
}

/// Parse resolved tokens; `None` when they do not form one expression.
pub fn parse(tokens: Vec<P>) -> Option<Expr> {
    if tokens.is_empty() || tokens.iter().any(|t| matches!(t, P::Fail { .. })) {
        return None;
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.expr(0)?;
    (parser.pos == parser.tokens.len()).then_some(expr)
}

struct Parser {
    tokens: Vec<P>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&P> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, offset: usize) -> Option<&P> {
        self.tokens.get(self.pos + offset)
    }

    fn next(&mut self) -> Option<P> {
        let p = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        p
    }

    fn expr(&mut self, min_bp: u8) -> Option<Expr> {
        let mut lhs = self.prefix()?;
        while let Some(p) = self.peek().cloned() {
            if is_postfix(&p) {
                if BP_POSTFIX < min_bp {
                    break;
                }
                self.pos += 1;
                lhs = postfix(lhs, p);
            } else if p == P::Conv {
                if BP_CONVERT < min_bp {
                    break;
                }
                self.pos += 1;
                let Some(P::Target(target)) = self.next() else {
                    return None;
                };
                lhs = Expr::Convert(Box::new(lhs), target);
            } else if let Some((left, right)) = infix_bp(&p) {
                if left < min_bp {
                    break;
                }
                self.pos += 1;
                if self.peek().is_none() {
                    // `5 +`: an operator with nothing after it is dropped.
                    break;
                }
                let rhs = self.expr(right)?;
                lhs = infix(p, lhs, rhs);
            } else if BP_POSTFIX < min_bp {
                break;
            } else if p == P::LParen {
                // `2(3)`, `x(2)`: juxtaposition with a group multiplies.
                let group = self.prefix()?;
                lhs = Expr::Bin(BinOp::Mul, Box::new(lhs), Box::new(group));
            } else if self.implicit_addition_ahead() {
                // `1 h 30 min`, `1 m 20 cm`.
                let rhs = self.expr(BP_POSTFIX)?;
                lhs = Expr::Bin(BinOp::Add, Box::new(lhs), Box::new(rhs));
            } else if p.starts_operand() {
                // `2 pi`, `x x`, `10 % 3`: a second value is dropped.
                self.expr(BP_POSTFIX)?;
            } else {
                break;
            }
        }
        Some(lhs)
    }

    /// The value so far ends in a unit and a number with a unit follows.
    fn implicit_addition_ahead(&self) -> bool {
        let after_unit = self
            .pos
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i))
            .is_some_and(|p| matches!(p, P::Unit(_)));
        after_unit
            && matches!(self.peek(), Some(P::Num(..)))
            && matches!(self.peek_at(1), Some(P::Unit(_)))
    }

    fn prefix(&mut self) -> Option<Expr> {
        // An operator that cannot be unary has no left operand here and is
        // dropped: `* 2`, `in * 2` (§2.3 [oracle]).
        while matches!(self.peek(), Some(P::Op(op)) if !matches!(op, BinOp::Add | BinOp::Sub))
            || matches!(self.peek(), Some(P::Pct(_)))
        {
            self.pos += 1;
        }
        Some(match self.next()? {
            P::Num(n, fmt) => Expr::Num(n, fmt),
            P::Clock {
                hour,
                minute,
                second,
                padded,
            } => Expr::Clock {
                hour,
                minute,
                second,
                padded,
            },
            P::Var(name) => Expr::Var(name),
            P::Pi => Expr::Pi,
            P::E => Expr::E,
            P::Prev => Expr::Prev,
            P::Sum => Expr::Sum,
            P::Avg => Expr::Avg,
            P::Date(word) => Expr::Date(word),
            P::Now => Expr::Now(None),
            P::Zone(tz) if self.peek() == Some(&P::Now) => {
                self.pos += 1;
                Expr::Now(Some(tz))
            }
            P::Op(BinOp::Sub) => self.negation()?,
            P::Op(BinOp::Add) => self.expr(BP_UNARY)?,
            P::LParen => {
                let inner = self.expr(0)?;
                // `(2; 3)` keeps the first value.
                while self.peek() == Some(&P::Sep) {
                    self.pos += 1;
                    self.expr(0)?;
                }
                if self.next()? != P::RParen {
                    return None;
                }
                inner
            }
            // `$-5` has no result; `-$5` does.
            P::CurrencyPrefix(_) if self.peek() == Some(&P::Op(BinOp::Sub)) => return None,
            P::CurrencyPrefix(c) => Expr::Currency(Box::new(self.expr(BP_POSTFIX)?), c),
            P::UnitPrefix(u) => Expr::Unit(Box::new(self.expr(BP_POSTFIX)?), u),
            P::MonthPrefix(month) => match self.next()? {
                P::Num(n, fmt) => Expr::Day(Box::new(Expr::Num(n, fmt)), month),
                _ => return None,
            },
            P::Func(f) => self.call(f)?,
            _ => return None,
        })
    }

    /// Unary minus. A run of minus signs straight before a number negates
    /// once (`--5` = -5), as the number extractor reads `-5` as one token;
    /// `-(-5)` = 5 (§2.3 [oracle]).
    fn negation(&mut self) -> Option<Expr> {
        let mut extra = 0;
        while self.peek() == Some(&P::Op(BinOp::Sub)) {
            self.pos += 1;
            extra += 1;
        }
        if matches!(self.peek(), Some(P::Num(..))) {
            return Some(Expr::Neg(Box::new(self.expr(BP_UNARY)?)));
        }
        let mut expr = self.expr(BP_UNARY)?;
        for _ in 0..=extra {
            expr = Expr::Neg(Box::new(expr));
        }
        Some(expr)
    }

    /// `f(x)`, `f x`, `log b (x)`. A function name followed by a minus
    /// sign without parentheses fails (`abs -5`); `log` and `root` take
    /// their second argument only in parentheses (§9 [oracle]).
    fn call(&mut self, func: Func) -> Option<Expr> {
        let two_part = matches!(func, Func::Log | Func::Root);
        let mut args = if self.peek() == Some(&P::LParen) {
            self.pos += 1;
            self.arguments()?
        } else if self.peek() == Some(&P::Op(BinOp::Sub)) {
            return None;
        } else if two_part {
            vec![self.prefix()?]
        } else {
            vec![self.expr(BP_UNARY)?]
        };
        if two_part && args.len() == 1 && self.peek() == Some(&P::LParen) {
            self.pos += 1;
            args.extend(self.arguments()?);
        }
        Some(Expr::Call(func, args))
    }

    /// Arguments after `(` up to the matching `)`, which is required.
    fn arguments(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.peek() == Some(&P::RParen) {
            self.pos += 1;
            return Some(args);
        }
        loop {
            args.push(self.expr(0)?);
            match self.next()? {
                P::Sep => continue,
                P::RParen => return Some(args),
                _ => return None,
            }
        }
    }
}

fn postfix(lhs: Expr, p: P) -> Expr {
    let lhs = Box::new(lhs);
    match p {
        P::Square => Expr::Bin(
            BinOp::Pow,
            lhs,
            Box::new(Expr::Num(Num::int(2), NumFmt::Decimal)),
        ),
        P::Cube => Expr::Bin(
            BinOp::Pow,
            lhs,
            Box::new(Expr::Num(Num::int(3), NumFmt::Decimal)),
        ),
        P::Unit(u) => Expr::Unit(lhs, u),
        P::CurrencySuffix(c) => Expr::Currency(lhs, c),
        P::Scale(n) => Expr::Scale(lhs, n),
        P::Percent => Expr::Percent(lhs),
        P::Meridiem(pm) => Expr::Meridiem(lhs, pm),
        P::Zone(tz) => Expr::InZone(lhs, tz),
        P::MonthSuffix(month) => Expr::Day(lhs, month),
        _ => *lhs,
    }
}

fn infix(p: P, lhs: Expr, rhs: Expr) -> Expr {
    match p {
        P::Op(op) => Expr::Bin(op, Box::new(lhs), Box::new(rhs)),
        P::Pct(op) => Expr::Pct(op, Box::new(lhs), Box::new(rhs)),
        _ => lhs,
    }
}
