//! The JavaScript side: one dedicated thread owns the QuickJS runtime and
//! its one shared context. It runs every script at load, reports what they
//! asked for as [`Event`]s, then serves extension function calls sent over
//! a channel until every [`ScriptFunction`] holding the sender is gone.
//!
//! A deadline guards every run: QuickJS's interrupt handler aborts a
//! script past it, so an endless loop costs one timeout, not the thread.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use numero_engine::{FunctionSpec, ScriptFunction, ScriptValue, UnitSpec};
use rquickjs::context::EvalOptions;
use rquickjs::function::Rest;
use rquickjs::{Array, Coerced, Context, Ctx, Exception, Function, Object, Runtime, Value};

use crate::convert;

/// How long one extension function call may run.
pub const CALL_TIMEOUT: Duration = Duration::from_millis(250);
/// How long one file may run at load.
pub const FILE_TIMEOUT: Duration = Duration::from_millis(1000);
/// After a call times out, further calls to that function fail at once
/// for this long, so a document calling it on every line stays usable.
pub const COOLDOWN: Duration = Duration::from_secs(2);
/// The error a timed-out call gives.
pub const TIMED_OUT: &str = "extension timed out";

const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const STACK_LIMIT: usize = 1024 * 1024;
/// Extra wait for a reply past the deadline, for the interrupt to land.
const REPLY_SLACK: Duration = Duration::from_millis(50);
/// Where the context keeps `addFunction`'s functions, by index.
const FUNCTIONS: &str = "__numeroFunctions";
const PRELUDE: &str = "Object.defineProperty(globalThis, '__numeroFunctions', \
     { value: [], enumerable: false, writable: false, configurable: false });";

/// A file to run: its relative name and its source, or why it could not
/// be read.
#[derive(Debug, Clone)]
pub struct Script {
    pub name: String,
    pub source: Result<String, String>,
}

/// What the scripts did at load, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Log(String),
    SetVariable(String, ScriptValue),
    AddUnit(UnitSpec),
    AddFunction(FunctionSpec, u32),
}

/// A loaded runtime: its events and the way to call its functions.
#[derive(Debug)]
pub struct Host {
    pub events: Vec<Event>,
    calls: Sender<Call>,
}

#[derive(Debug)]
struct Call {
    index: u32,
    args: Vec<ScriptValue>,
    deadline: Instant,
    reply: SyncSender<Result<ScriptValue, String>>,
}

/// Collects events while loading; `None` afterwards, so `log()` inside a
/// function called on every keystroke does not pile up.
type Events = Rc<RefCell<Option<Vec<Event>>>>;

/// Start the JavaScript thread and run `scripts` on it.
pub fn start(scripts: Vec<Script>) -> Result<Host, String> {
    let budget = FILE_TIMEOUT * u32::try_from(scripts.len()).unwrap_or(u32::MAX);
    let (calls, call_rx) = mpsc::channel();
    let (events_tx, events_rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("numero-extensions".into())
        .spawn(move || serve(scripts, &events_tx, &call_rx))
        .map_err(|e| format!("could not start the extensions thread: {e}"))?;
    let events = events_rx
        .recv_timeout(budget + Duration::from_secs(1))
        .map_err(|_| "extensions did not finish loading".to_string())??;
    Ok(Host { events, calls })
}

impl Host {
    /// Extension function `index` as an engine [`ScriptFunction`].
    pub fn function(&self, index: u32) -> ScriptFunction {
        let calls = self.calls.clone();
        let cooling_until: Mutex<Option<Instant>> = Mutex::new(None);
        Arc::new(move |args: &[ScriptValue]| {
            let mut until = cooling_until.lock().unwrap_or_else(PoisonError::into_inner);
            if until.is_some_and(|t| Instant::now() < t) {
                return Err(TIMED_OUT.into());
            }
            let result = call(&calls, index, args);
            *until = matches!(&result, Err(e) if e == TIMED_OUT).then(|| Instant::now() + COOLDOWN);
            result
        })
    }
}

fn call(calls: &Sender<Call>, index: u32, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let deadline = Instant::now() + CALL_TIMEOUT;
    let (reply, reply_rx) = mpsc::sync_channel(1);
    let request = Call {
        index,
        args: args.to_vec(),
        deadline,
        reply,
    };
    calls
        .send(request)
        .map_err(|_| "extensions are not running".to_string())?;
    match reply_rx.recv_timeout(CALL_TIMEOUT + REPLY_SLACK) {
        Ok(result) => result,
        // Dropped unanswered: the deadline passed while it queued.
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Err(TIMED_OUT.into()),
    }
}

fn serve(
    scripts: Vec<Script>,
    events_tx: &SyncSender<Result<Vec<Event>, String>>,
    calls: &Receiver<Call>,
) {
    let deadline = Rc::new(Cell::new(None::<Instant>));
    let (runtime, context) = match runtime(&deadline) {
        Ok(pair) => pair,
        Err(error) => {
            let _ = events_tx.send(Err(format!("could not start JavaScript: {error}")));
            return;
        }
    };
    let events: Events = Rc::new(RefCell::new(Some(Vec::new())));
    context.with(|ctx| {
        if let Err(error) = install(&ctx, &events) {
            push(&events, Event::Log(format!("Error: {error}")));
        }
        for script in scripts {
            push(&events, Event::Log(format!("Loading {}", script.name)));
            match script.source {
                Ok(source) => run_file(&ctx, &deadline, &events, &script.name, source),
                Err(error) => push(&events, Event::Log(format!("Error: {error}"))),
            }
        }
    });
    let loaded = events.borrow_mut().take().unwrap_or_default();
    if events_tx.send(Ok(loaded)).is_err() {
        return;
    }
    while let Ok(request) = calls.recv() {
        if Instant::now() >= request.deadline {
            continue;
        }
        deadline.set(Some(request.deadline));
        let result = context.with(|ctx| invoke(&ctx, request.index, &request.args));
        deadline.set(None);
        let result = match result {
            Err(_) if Instant::now() >= request.deadline => Err(TIMED_OUT.into()),
            other => other,
        };
        let _ = request.reply.send(result);
    }
    drop(context);
    drop(runtime);
}

fn runtime(deadline: &Rc<Cell<Option<Instant>>>) -> rquickjs::Result<(Runtime, Context)> {
    let runtime = Runtime::new()?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(STACK_LIMIT);
    let deadline = Rc::clone(deadline);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        deadline.get().is_some_and(|t| Instant::now() >= t)
    })));
    let context = Context::full(&runtime)?;
    Ok((runtime, context))
}

fn push(events: &Events, event: Event) {
    if let Some(events) = events.borrow_mut().as_mut() {
        events.push(event);
    }
}

fn run_file(
    ctx: &Ctx<'_>,
    deadline: &Cell<Option<Instant>>,
    events: &Events,
    name: &str,
    source: String,
) {
    let mut options = EvalOptions::default();
    options.strict = false;
    options.filename = Some(name.to_string());
    let end = Instant::now() + FILE_TIMEOUT;
    deadline.set(Some(end));
    let result = ctx.eval_with_options::<(), _>(source, options);
    deadline.set(None);
    if result.is_err() {
        let timed_out = Instant::now() >= end;
        push(events, Event::Log(describe_error(ctx, timed_out)));
    }
}

/// The pending exception as Numi's "Error on line N: message".
fn describe_error(ctx: &Ctx<'_>, timed_out: bool) -> String {
    let thrown = ctx.catch();
    let (message, stack) = match thrown
        .clone()
        .into_object()
        .and_then(Exception::from_object)
    {
        Some(exception) => (
            exception.message().unwrap_or_default(),
            exception.stack().unwrap_or_default(),
        ),
        None => (coerce(thrown), String::new()),
    };
    let message = if timed_out {
        format!("ran longer than {} ms", FILE_TIMEOUT.as_millis())
    } else {
        message
    };
    match line_in(&stack) {
        Some(line) => format!("Error on line {line}: {message}"),
        None => format!("Error: {message}"),
    }
}

/// The line of the innermost script frame (`at f (file:LINE:COL)` or
/// `at file:LINE:COL`), skipping native ones; Numi reports the line the
/// exception was raised on, wherever that is.
fn line_in(stack: &str) -> Option<u32> {
    stack.lines().find_map(|frame| {
        let location = frame.trim().strip_prefix("at ")?;
        let location = match location.rfind('(') {
            Some(open) => location[open + 1..].trim_end_matches(')'),
            None => location,
        };
        let mut parts = location.rsplitn(3, ':');
        let _column = parts.next()?;
        let line = parts.next()?.parse().ok()?;
        parts.next().map(|_| line)
    })
}

fn coerce(value: Value<'_>) -> String {
    value
        .get::<Coerced<String>>()
        .map(|c| c.0)
        .unwrap_or_else(|_| "unknown error".into())
}

fn throw(ctx: &Ctx<'_>, message: &str) -> rquickjs::Error {
    Exception::throw_type(ctx, message)
}

/// The `numi` object and the global `log`.
fn install<'js>(ctx: &Ctx<'js>, events: &Events) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(PRELUDE)?;
    let numi = Object::new(ctx.clone())?;

    let sink = Rc::clone(events);
    let set_variable = move |ctx: Ctx<'js>, name: String, value: Value<'js>| {
        let value =
            convert::script_value(&value).map_err(|e| throw(&ctx, &format!("setVariable: {e}")))?;
        push(&sink, Event::SetVariable(name, value));
        rquickjs::Result::Ok(())
    };
    numi.set("setVariable", Function::new(ctx.clone(), set_variable)?)?;

    let sink = Rc::clone(events);
    let add_unit = move |ctx: Ctx<'js>, dict: Object<'js>| {
        let spec = convert::unit_spec(&dict).map_err(|e| throw(&ctx, &format!("addUnit: {e}")))?;
        push(&sink, Event::AddUnit(spec));
        rquickjs::Result::Ok(())
    };
    numi.set("addUnit", Function::new(ctx.clone(), add_unit)?)?;

    let sink = Rc::clone(events);
    let add_function = move |ctx: Ctx<'js>, dict: Object<'js>, function: Function<'js>| {
        let spec =
            convert::function_spec(&dict).map_err(|e| throw(&ctx, &format!("addFunction: {e}")))?;
        let functions: Array<'js> = ctx.globals().get(FUNCTIONS)?;
        let index = u32::try_from(functions.len())
            .map_err(|_| throw(&ctx, "addFunction: too many functions"))?;
        functions.set(index as usize, function)?;
        push(&sink, Event::AddFunction(spec, index));
        rquickjs::Result::Ok(())
    };
    numi.set("addFunction", Function::new(ctx.clone(), add_function)?)?;

    let sink = Rc::clone(events);
    let log = move |parts: Rest<Coerced<String>>| {
        let line = parts
            .0
            .into_iter()
            .map(|c| c.0)
            .collect::<Vec<_>>()
            .join(" ");
        push(&sink, Event::Log(line));
    };
    let globals = ctx.globals();
    globals.set("numi", numi)?;
    globals.set("log", Function::new(ctx.clone(), log)?)?;
    Ok(())
}

/// Call extension function `index` with Numi value dicts.
fn invoke(ctx: &Ctx<'_>, index: u32, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let prepared = (|| {
        let functions: Array<'_> = ctx.globals().get(FUNCTIONS)?;
        let function: Function<'_> = functions.get(index as usize)?;
        let values = Array::new(ctx.clone())?;
        for (i, arg) in args.iter().enumerate() {
            values.set(i, convert::value_dict(ctx, arg)?)?;
        }
        rquickjs::Result::Ok((function, values))
    })();
    let (function, values) = prepared.map_err(|e| e.to_string())?;
    match function.call::<_, Value<'_>>((values,)) {
        Ok(result) => convert::script_value(&result).map_err(|e| format!("extension result: {e}")),
        Err(rquickjs::Error::Exception) => Err(exception_message(ctx)),
        Err(error) => Err(error.to_string()),
    }
}

fn exception_message(ctx: &Ctx<'_>) -> String {
    let thrown = ctx.catch();
    match thrown
        .clone()
        .into_object()
        .and_then(Exception::from_object)
    {
        Some(exception) => exception.message().unwrap_or_else(|| "error".into()),
        None => coerce(thrown),
    }
}

#[cfg(test)]
mod tests {
    use super::line_in;

    #[test]
    fn finds_the_innermost_script_line() {
        let stack = "    at addUnit (native)\n    at <eval> (sub/a.js:7:3)\n";
        assert_eq!(line_in(stack), Some(7));
        assert_eq!(line_in("    at b.js:3:9\n"), Some(3));
        assert_eq!(line_in("    at f (native)\n"), None);
    }
}
