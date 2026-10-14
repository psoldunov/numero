//! From tokens to an expression tree, in two passes.
//!
//! 1. [`resolve`] picks one meaning for every phrase from its neighbours and
//!    drops what does not take part in the calculation: free text, a unit
//!    word with no number before it, `and` with nothing to join. That is how
//!    `$9 for coffee + $3 tip` reduces to `$9 + $3`.
//! 2. [`parse`] runs a Pratt parser over what is left with Numi's
//!    priorities (§2.2): `+ −` and the percent phrases at 100; `× ÷ mod &
//!    | xor << >>` at 200; `^` and `in` at 300; all left-associative.
//!    Unary minus binds tightest, then postfix units, `%`, scales, `²`.
//!    There is no implicit multiplication and no `!`.

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
    Clock { hour: u32, minute: u32, second: u32 },
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Pct(PctOp, Box<Expr>, Box<Expr>),
    Percent(Box<Expr>),
    Unit(Box<Expr>, UnitRef),
    Currency(Box<Expr>, u16),
    Scale(Box<Expr>, Num),
    Meridiem(Box<Expr>, bool),
    InZone(Box<Expr>, Tz),
    Call(Func, Vec<Expr>),
    Convert(Box<Expr>, Target),
}

/// A token with exactly one meaning, ready for the Pratt parser.
#[derive(Debug, Clone, PartialEq)]
pub enum P {
    Num(Num, NumFmt),
    Clock { hour: u32, minute: u32, second: u32 },
    Meridiem(bool),
    Var(String),
    Op(BinOp),
    Square,
    Cube,
    LParen,
    RParen,
    Sep,
    Unit(UnitRef),
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
        )
    }

    fn is_time_like(&self) -> bool {
        matches!(self, P::Clock { .. } | P::Meridiem(_) | P::Now | P::Date(_))
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
        _ => None,
    }
}

/// The tokens that take part in the calculation, and a highlight class for
/// every input token (same order as `tokens`).
pub fn resolve(tokens: &[Token]) -> (Vec<P>, Vec<TokenClass>) {
    let mut out: Vec<P> = Vec::with_capacity(tokens.len());
    let mut classes = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens[i + 1..]
            .iter()
            .map(|t| &t.tok)
            .find(|t| !matches!(t, Tok::Ident | Tok::Comment));
        let (p, class) = resolve_one(&token.tok, out.last(), next);
        classes.push(class);
        if let Some(p) = p {
            out.push(p);
        }
    }
    (out, classes)
}

fn resolve_one(tok: &Tok, prev: Option<&P>, next: Option<&Tok>) -> (Option<P>, TokenClass) {
    use TokenClass as C;
    match tok {
        Tok::Num(n, fmt) => (Some(P::Num(*n, *fmt)), C::Number),
        Tok::Time {
            hour,
            minute,
            second,
        } => (
            Some(P::Clock {
                hour: *hour,
                minute: *minute,
                second: *second,
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
        Tok::Phrase(meanings) => resolve_phrase(meanings, prev, next),
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

fn resolve_phrase(
    meanings: &[Meaning],
    prev: Option<&P>,
    next: Option<&Tok>,
) -> (Option<P>, TokenClass) {
    if matches!(prev, Some(P::Conv))
        && let Some(target) = meanings.iter().find_map(|m| as_target(*m))
    {
        return (Some(P::Target(target)), target_class(target));
    }
    let next_is_now = matches!(next, Some(Tok::Phrase(ms)) if ms.contains(&Meaning::Now));
    let after_time = prev.is_some_and(P::is_time_like);
    if (next_is_now || after_time)
        && let Some(Meaning::Zone(tz)) = meanings.iter().find(|m| matches!(m, Meaning::Zone(_)))
    {
        return (Some(P::Zone(*tz)), TokenClass::Keyword);
    }
    let prev_value = prev.is_some_and(P::ends_value);
    let next_value = next.is_some_and(starts_value);
    for meaning in meanings.iter().filter(|m| !matches!(m, Meaning::Zone(_))) {
        if let Some(resolved) = resolve_meaning(*meaning, prev, prev_value, next_value, next) {
            return resolved;
        }
    }
    (None, TokenClass::Text)
}

fn resolve_meaning(
    meaning: Meaning,
    prev: Option<&P>,
    prev_value: bool,
    next_value: bool,
    next: Option<&Tok>,
) -> Option<(Option<P>, TokenClass)> {
    use TokenClass as C;
    let keep = |p: P, class: C| Some((Some(p), class));
    match meaning {
        Meaning::Unit(u) if prev_value => keep(P::Unit(u), C::Unit),
        Meaning::Currency(c) if prev_value => keep(P::CurrencySuffix(c), C::Currency),
        Meaning::Currency(c) if next_value => keep(P::CurrencyPrefix(c), C::Currency),
        Meaning::Scale(n) if matches!(prev, Some(P::Num(..))) => keep(P::Scale(n), C::Number),
        Meaning::Percent if prev_value => keep(P::Percent, C::Operator),
        Meaning::Pct(op)
            if next_value
                && if op.needs_percent_left() {
                    matches!(prev, Some(P::Percent))
                } else {
                    prev_value
                } =>
        {
            keep(P::Pct(op), C::Keyword)
        }
        Meaning::Conversion if prev_value && next_is_target(next) => keep(P::Conv, C::Keyword),
        Meaning::Plus if prev_value && next_value => keep(P::Op(BinOp::Add), C::Operator),
        Meaning::Minus if prev_value && next_value => keep(P::Op(BinOp::Sub), C::Operator),
        Meaning::Times if prev_value && next_value => keep(P::Op(BinOp::Mul), C::Operator),
        Meaning::Divide if prev_value && next_value => keep(P::Op(BinOp::Div), C::Operator),
        Meaning::Mod if prev_value && next_value => keep(P::Op(BinOp::Mod), C::Operator),
        Meaning::Xor if prev_value && next_value => keep(P::Op(BinOp::Xor), C::Operator),
        Meaning::Func(f) if next_value => keep(P::Func(f), C::Function),
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
    )
}

/// Parse resolved tokens; `None` when they do not form one expression.
pub fn parse(tokens: Vec<P>) -> Option<Expr> {
    if tokens.is_empty() {
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
                let rhs = self.expr(right)?;
                lhs = infix(p, lhs, rhs);
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn prefix(&mut self) -> Option<Expr> {
        Some(match self.next()? {
            P::Num(n, fmt) => Expr::Num(n, fmt),
            P::Clock {
                hour,
                minute,
                second,
            } => Expr::Clock {
                hour,
                minute,
                second,
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
            P::Op(BinOp::Sub) => Expr::Neg(Box::new(self.expr(BP_UNARY)?)),
            P::Op(BinOp::Add) => self.expr(BP_UNARY)?,
            P::LParen => {
                let inner = self.expr(0)?;
                match self.peek() {
                    Some(P::RParen) => self.pos += 1,
                    None => {}
                    Some(_) => return None,
                }
                inner
            }
            P::CurrencyPrefix(c) => Expr::Currency(Box::new(self.expr(BP_POSTFIX)?), c),
            P::Func(f) => self.call(f)?,
            _ => return None,
        })
    }

    fn call(&mut self, func: Func) -> Option<Expr> {
        if self.peek() == Some(&P::LParen) {
            self.pos += 1;
            let args = self.arguments()?;
            return Some(Expr::Call(func, args));
        }
        let first = self.expr(BP_UNARY)?;
        let mut args = vec![first];
        if matches!(func, Func::Log | Func::Root) && self.peek() == Some(&P::LParen) {
            self.pos += 1;
            args.extend(self.arguments()?);
        }
        Some(Expr::Call(func, args))
    }

    /// Arguments after `(` up to the matching `)`.
    fn arguments(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.peek() == Some(&P::RParen) {
            self.pos += 1;
            return Some(args);
        }
        loop {
            args.push(self.expr(0)?);
            match self.next() {
                Some(P::Sep) => continue,
                Some(P::RParen) | None => return Some(args),
                Some(_) => return None,
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
