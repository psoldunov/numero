//! A document, line by line (§5): headers, labels, comments, variables,
//! `prev`, `sum`/`avg` blocks and the grand total.
//!
//! Every line's result (or its absence) feeds the next: `prev` is the line
//! directly above, `sum` walks up from there to the first line without a
//! result, and the grand total adds every result up (§5 [oracle]). A line
//! whose calculation fails (NaN), or a result the total cannot add,
//! empties the grand total for the rest of the document; a line Numi gives
//! an empty value (`log 100`, `5 m in foo`) starts it over; blank,
//! comment and unparsable lines leave it alone.
//!
//! Variable names ignore case and may contain spaces (`Total cost = 5`,
//! then `total cost * 2`).

use std::collections::HashMap;

use crate::eval::{Scope, eval, quantity, unit_ref};
use crate::ext::ExtFunction;
use crate::lexer::{NumberStyle, Sym, Tok, Token, lex, normalize_name};
use crate::num::Num;
use crate::parser::{BinOp, Expr, P, parse, resolve};
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

/// What a line does to the blocks and the grand total.
#[derive(Debug, Clone, Copy)]
enum Effect {
    Value(Value),
    /// No value at all: blank, comment, header, unparsable text.
    Skip,
    /// Numi's empty value: the grand total starts over.
    Reset,
    /// NaN: the grand total stays empty.
    Break,
}

/// The running grand total.
#[derive(Debug, Clone, Copy)]
enum Total {
    Empty,
    Sum(Value),
    /// A line failed or could not be added: Numi's NaN.
    Broken,
}

/// What carries from one line to the next.
#[derive(Debug)]
struct State {
    vars: HashMap<String, Value>,
    /// The keys of `vars`, for the lexer.
    names: Vec<String>,
    block: Vec<Value>,
    prev: Option<Value>,
    env: UnitEnv,
    grand_total: Total,
}

pub fn evaluate(document: &str, inputs: &Inputs<'_>) -> Outcome {
    let ppi = setting(inputs.config.ppi);
    let mut vars: HashMap<String, Value> = HashMap::new();
    vars.insert("r".into(), Value::plain(Num::exact(GAS_CONSTANT)));
    vars.insert("ppi".into(), Value::plain(ppi));
    vars.extend(
        inputs
            .globals
            .iter()
            .map(|(name, value)| (normalize_name(name), *value)),
    );
    let names = vars.keys().cloned().collect();
    let mut state = State {
        vars,
        names,
        block: Vec::new(),
        prev: None,
        env: UnitEnv {
            ppi,
            em_px: setting(inputs.config.em_px),
        },
        grand_total: Total::Empty,
    };
    let lines = document
        .split('\n')
        .map(|line| {
            let (result, effect) =
                evaluate_line(line.strip_suffix('\r').unwrap_or(line), inputs, &mut state);
            state.prev = None;
            match effect {
                Effect::Value(value) => {
                    state.block.push(value);
                    add_to_total(&mut state, value, inputs);
                    state.prev = Some(value);
                }
                Effect::Skip => state.block.clear(),
                Effect::Reset => {
                    state.block.clear();
                    state.grand_total = Total::Empty;
                }
                Effect::Break => {
                    state.block.clear();
                    state.grand_total = Total::Broken;
                }
            }
            result
        })
        .collect();
    Outcome {
        lines,
        grand_total: match state.grand_total {
            Total::Sum(value) => Some(value),
            Total::Empty | Total::Broken => None,
        },
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

/// The grand total adds every result with `+`; one that cannot be added
/// breaks it for good (§5 [oracle]).
fn add_to_total(state: &mut State, value: Value, inputs: &Inputs<'_>) {
    state.grand_total = match state.grand_total {
        Total::Empty => Total::Sum(value),
        Total::Sum(total) => crate::eval::binary(BinOp::Add, total, value, &scope(inputs, state))
            .map_or(Total::Broken, Total::Sum),
        Total::Broken => Total::Broken,
    };
}

fn result(kind: LineKind, spans: Vec<Span>, effect: Effect) -> (LineResult, Effect) {
    (
        LineResult {
            kind,
            display: None,
            number: None,
            spans,
        },
        effect,
    )
}

fn evaluate_line(line: &str, inputs: &Inputs<'_>, state: &mut State) -> (LineResult, Effect) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return result(LineKind::Empty, Vec::new(), Effect::Skip);
    }
    let lead = line.len() - line.trim_start().len();
    if trimmed.starts_with('#') {
        let span = Span {
            range: lead..line.trim_end().len(),
            class: TokenClass::Header,
        };
        return result(LineKind::Header, vec![span], Effect::Skip);
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
    let style = NumberStyle {
        decimal: inputs.config.decimal_separator,
    };
    let tokens = lex(line, start, inputs.vocab, style, &state.names);
    let assignment = split_assignment(line, &tokens);
    let body = &tokens[assignment.as_ref().map_or(0, |a| a.operator + 1)..];
    let (resolved, classes) = resolve(body);
    if let Some(a) = &assignment {
        let name_span = tokens[0].span.start..tokens[a.operator - 1].span.end;
        spans.push(Span {
            range: name_span,
            class: TokenClass::Variable,
        });
        spans.push(Span {
            range: tokens[a.operator].span.clone(),
            class: TokenClass::Operator,
        });
    }
    spans.extend(body.iter().zip(classes).map(|(t, class)| Span {
        range: t.span.clone(),
        class,
    }));
    if resolved.is_empty() {
        return result(LineKind::Comment, spans, Effect::Skip);
    }
    let resets = resolved
        .iter()
        .any(|p| matches!(p, P::Fail { resets: true }));
    let Some(expr) = parse(resolved) else {
        let effect = if resets { Effect::Reset } else { Effect::Skip };
        return result(LineKind::Comment, spans, effect);
    };
    let outcome = eval(&expr, &scope(inputs, state));
    match outcome {
        Ok(value) => {
            if let Some(name) = assignment.and_then(|a| a.name) {
                assign(state, name, value, inputs.vocab);
            }
            let line_result = LineResult {
                kind: LineKind::Value,
                display: Some(value.display(inputs.vocab, inputs.config, inputs.ctx)),
                number: value.number(),
                spans,
            };
            (line_result, Effect::Value(value))
        }
        // `prev` with nothing above is empty rather than a failure, so
        // it leaves the grand total alone (§5 [oracle]).
        Err(_) if expr == Expr::Prev => result(LineKind::Comment, spans, Effect::Skip),
        Err(e) if e.empty => result(LineKind::Error, spans, Effect::Reset),
        Err(_) => result(LineKind::Error, spans, Effect::Break),
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

/// `name = …`: where the assignment operator is, and the variable name
/// if it is one Numi stores (letters, digits, `_` and spaces, with at least
/// one letter or `_`). The line shows the right-hand side either way:
/// `5 = 3` shows 3 (§5 [oracle]).
struct Assignment {
    operator: usize,
    name: Option<String>,
}

/// `name = …`, `name equal …`, `name is …`.
fn split_assignment(line: &str, tokens: &[Token]) -> Option<Assignment> {
    let operator = tokens.iter().position(|t| match &t.tok {
        Tok::Sym(Sym::Assign) => true,
        Tok::Phrase(meanings) => meanings.contains(&Meaning::Assign),
        _ => false,
    })?;
    if operator == 0 || operator + 1 >= tokens.len() {
        return None;
    }
    let text = &line[tokens[0].span.start..tokens[operator - 1].span.end];
    let valid = text
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c.is_whitespace())
        && text.chars().any(|c| c.is_alphabetic() || c == '_');
    Some(Assignment {
        operator,
        name: valid.then(|| normalize_name(text)),
    })
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
            store(state, name, value);
        }
        _ => store(state, name, value),
    }
}

fn store(state: &mut State, name: String, value: Value) {
    if !state.vars.contains_key(&name) {
        state.names.push(name.clone());
    }
    state.vars.insert(name, value);
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
