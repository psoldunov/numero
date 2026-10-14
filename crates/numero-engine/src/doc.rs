//! A document, line by line (§5): headers, labels, comments, variables,
//! `prev`, `sum`/`avg` blocks and the grand total.
//!
//! Every line's result (or its absence) feeds the next: `prev` is the line
//! directly above, `sum` walks up from there to the first line without a
//! result, and the grand total adds every result that can be added.

use std::collections::HashMap;

use crate::eval::{Scope, eval, quantity, unit_ref};
use crate::ext::ExtFunction;
use crate::lexer::{NumberStyle, Sym, Tok, Token, lex};
use crate::num::Num;
use crate::parser::{BinOp, parse, resolve};
use crate::units::{Dim, UnitEnv, UnitTable};
use crate::value::{Unit, Value};
use crate::vocab::{Meaning, Vocab};
use crate::{Config, EvalContext, LineKind, LineResult, Rates, Span, TokenClass};

/// CSS defaults before a document sets `ppi` or `em` (§1.5).
pub const DEFAULT_ENV: UnitEnv = UnitEnv {
    ppi: Num::Float(96.0),
    em_px: Num::Float(16.0),
};
/// The molar gas constant, Numi's `r`.
const GAS_CONSTANT: &str = "8.3144598";

pub struct Inputs<'a> {
    pub config: &'a Config,
    pub rates: &'a Rates,
    pub vocab: &'a Vocab,
    pub ctx: &'a EvalContext,
    /// Extension variables, already resolved.
    pub globals: Vec<(String, Value)>,
    pub functions: &'a [ExtFunction],
}

/// Per-line results plus the running grand total.
pub struct Outcome {
    pub lines: Vec<LineResult>,
    pub grand_total: Option<Value>,
}

/// What carries from one line to the next.
#[derive(Debug)]
struct State {
    vars: HashMap<String, Value>,
    block: Vec<Value>,
    prev: Option<Value>,
    env: UnitEnv,
    grand_total: Option<Value>,
}

pub fn evaluate(document: &str, inputs: &Inputs<'_>) -> Outcome {
    let ppi = setting(inputs.config.ppi);
    let mut vars: HashMap<String, Value> = HashMap::new();
    vars.insert("r".into(), Value::plain(Num::exact(GAS_CONSTANT)));
    vars.insert("ppi".into(), Value::plain(ppi));
    vars.extend(inputs.globals.iter().cloned());
    let mut state = State {
        vars,
        block: Vec::new(),
        prev: None,
        env: UnitEnv {
            ppi,
            em_px: setting(inputs.config.em_px),
        },
        grand_total: None,
    };
    let lines = document
        .split('\n')
        .map(|line| {
            let result = evaluate_line(line.strip_suffix('\r').unwrap_or(line), inputs, &mut state);
            match result.1 {
                Some(value) => {
                    state.block.push(value);
                    add_to_total(&mut state, value, inputs);
                }
                None => state.block.clear(),
            }
            state.prev = result.1;
            result.0
        })
        .collect();
    Outcome {
        lines,
        grand_total: state.grand_total,
    }
}

fn setting(value: f64) -> Num {
    Num::parse(&value.to_string()).unwrap_or(Num::Float(value))
}

fn scope<'a>(inputs: &'a Inputs<'_>, state: &'a State) -> Scope<'a> {
    Scope {
        rates: inputs.rates,
        vocab: inputs.vocab,
        ctx: inputs.ctx,
        vars: &state.vars,
        prev: state.prev.as_ref(),
        block: &state.block,
        env: state.env,
        functions: inputs.functions,
    }
}

/// The grand total adds every result it can; additions that fail are
/// skipped (§5).
fn add_to_total(state: &mut State, value: Value, inputs: &Inputs<'_>) {
    let next = match state.grand_total {
        None => Some(value),
        Some(total) => crate::eval::binary(BinOp::Add, total, value, &scope(inputs, state)).ok(),
    };
    if next.is_some() {
        state.grand_total = next;
    }
}

fn result(kind: LineKind, spans: Vec<Span>) -> (LineResult, Option<Value>) {
    (
        LineResult {
            kind,
            display: None,
            number: None,
            spans,
        },
        None,
    )
}

fn evaluate_line(
    line: &str,
    inputs: &Inputs<'_>,
    state: &mut State,
) -> (LineResult, Option<Value>) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return result(LineKind::Empty, Vec::new());
    }
    let lead = line.len() - line.trim_start().len();
    if trimmed.starts_with('#') {
        let span = Span {
            range: lead..line.trim_end().len(),
            class: TokenClass::Header,
        };
        return result(LineKind::Header, vec![span]);
    }
    let mut spans = Vec::new();
    let mut start = 0;
    if let Some(end) = label_end(line) {
        spans.push(Span {
            range: lead..end,
            class: TokenClass::Label,
        });
        start = end;
    }
    let vars = &state.vars;
    let is_var = |name: &str| vars.contains_key(name);
    let style = NumberStyle {
        decimal: inputs.config.decimal_separator,
    };
    let tokens = lex(line, start, inputs.vocab, style, &is_var);
    let assignment = split_assignment(line, &tokens);
    let body = &tokens[assignment.as_ref().map_or(0, |_| 2)..];
    let (resolved, classes) = resolve(body);
    if let Some(name_token) = assignment.as_ref().and(tokens.first()) {
        spans.push(Span {
            range: name_token.span.clone(),
            class: TokenClass::Variable,
        });
        spans.push(Span {
            range: tokens[1].span.clone(),
            class: TokenClass::Operator,
        });
    }
    spans.extend(body.iter().zip(classes).map(|(t, class)| Span {
        range: t.span.clone(),
        class,
    }));
    if resolved.is_empty() {
        return result(LineKind::Comment, spans);
    }
    let Some(expr) = parse(resolved) else {
        return result(LineKind::Comment, spans);
    };
    let outcome = eval(&expr, &scope(inputs, state));
    match outcome {
        Ok(value) => {
            if let Some(name) = assignment {
                assign(state, name, value, inputs.vocab);
            }
            let line_result = LineResult {
                kind: LineKind::Value,
                display: Some(value.display(inputs.vocab, inputs.config, inputs.ctx)),
                number: value.number(),
                spans,
            };
            (line_result, Some(value))
        }
        Err(_) => result(LineKind::Error, spans),
    }
}

/// `Label: 5` (§2.1): everything up to and including the last colon is a
/// label, except colons inside clock times (`2:30`) or after a comment.
fn label_end(line: &str) -> Option<usize> {
    let code_end = [line.find("//"), line.find('"')]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(line.len());
    let bytes = line.as_bytes();
    let in_clock = |i: usize| {
        i > 0 && bytes[i - 1].is_ascii_digit() && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
    };
    let colon = line[..code_end]
        .char_indices()
        .filter(|&(i, c)| c == ':' && !in_clock(i))
        .map(|(i, _)| i)
        .next_back()?;
    line[..colon]
        .chars()
        .any(char::is_alphanumeric)
        .then_some(colon + 1)
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// `name = …`, `name equal …`, `name is …`: the variable name, if the line
/// assigns one.
fn split_assignment(line: &str, tokens: &[Token]) -> Option<String> {
    let [first, second, _, ..] = tokens else {
        return None;
    };
    let name = &line[first.span.clone()];
    let assigns = match &second.tok {
        Tok::Sym(Sym::Assign) => true,
        Tok::Phrase(meanings) => meanings.contains(&Meaning::Assign),
        _ => false,
    };
    (assigns && is_identifier(name)).then(|| name.to_string())
}

/// Store a variable. `ppi` and `em` also retune CSS units for the lines
/// below; `em` stays usable as a unit, so it is not stored as a variable.
fn assign(state: &mut State, name: String, value: Value, vocab: &Vocab) {
    match name.as_str() {
        "em" => {
            if let Some(px) = pixels(value, state.env, vocab) {
                state.env.em_px = px;
            }
        }
        "ppi" => {
            if let Ok(q) = quantity(value) {
                state.env.ppi = q.n;
            }
            state.vars.insert(name, value);
        }
        _ => {
            state.vars.insert(name, value);
        }
    }
}

fn pixels(value: Value, env: UnitEnv, vocab: &Vocab) -> Option<Num> {
    let q = quantity(value).ok()?;
    match q.unit {
        Unit::None => Some(q.n),
        Unit::Phys(u) if vocab.dim(u) == Dim::Length => {
            vocab.convert(q.n, u, unit_ref(vocab, "pixel")?, env)
        }
        _ => None,
    }
}
