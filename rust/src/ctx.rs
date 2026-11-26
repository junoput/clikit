//! The reader of the output: the mode, the colours and symbols that follow from it, and the
//! three streams. Everything else in the crate prints and reads through this module, which
//! is what lets [`crate::testing`] capture a run.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::Duration;

/// Who the output is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A person at a terminal: colours, symbols, animation, key-driven prompts.
    Rich,
    /// A pipe, a file, CI: the same words without escape codes or animation.
    Plain,
    /// An LLM agent: only what is needed, in as few tokens as possible.
    Agent,
}

impl Mode {
    /// The name used by `CLI_MODE`: `rich`, `plain` or `agent`.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Rich => "rich",
            Mode::Plain => "plain",
            Mode::Agent => "agent",
        }
    }

    /// The reverse of [`Mode::as_str`].
    pub fn parse(name: &str) -> Option<Mode> {
        match name {
            "rich" => Some(Mode::Rich),
            "plain" => Some(Mode::Plain),
            "agent" => Some(Mode::Agent),
            _ => None,
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const BOLD: &str = "\x1b[1m";
pub(crate) const DIM: &str = "\x1b[2m";
pub(crate) const RED: &str = "\x1b[31m";
pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const YELLOW: &str = "\x1b[33m";
pub(crate) const BLUE: &str = "\x1b[34m";
pub(crate) const MAGENTA: &str = "\x1b[35m";
pub(crate) const CYAN: &str = "\x1b[36m";
pub(crate) const GRAY: &str = "\x1b[90m";

/// Environment variables that common agent harnesses set.
const HARNESSES: [&str; 7] =
    ["CLAUDECODE", "CURSOR_AGENT", "GEMINI_CLI", "CODEX_SANDBOX", "OPENCODE", "AI_AGENT", "AGENT"];

#[derive(Clone, Copy, Debug)]
pub(crate) struct Theme {
    pub mode: Mode,
    pub color: bool,
    /// Unicode symbols; only ever true in rich mode.
    pub utf: bool,
}

impl Theme {
    fn detect(env: &dyn Fn(&str) -> Option<String>, tty: bool, mode: Option<Mode>, color: Option<bool>) -> Theme {
        let mode =
            mode.or_else(|| env("CLI_MODE").and_then(|m| Mode::parse(&m))).unwrap_or_else(|| Theme::auto(env, tty));
        let locale =
            env("LC_ALL").or_else(|| env("LC_CTYPE")).or_else(|| env("LANG")).unwrap_or_default().to_ascii_lowercase();
        let utf = locale.contains("utf-8") || locale.contains("utf8") || (cfg!(windows) && env("WT_SESSION").is_some());
        Theme {
            mode,
            color: mode == Mode::Rich && color.unwrap_or_else(|| env("NO_COLOR").is_none()),
            utf: mode == Mode::Rich && utf,
        }
    }

    fn auto(env: &dyn Fn(&str) -> Option<String>, tty: bool) -> Mode {
        match env("CLI_AGENT").as_deref() {
            Some("1" | "true" | "yes") => return Mode::Agent,
            Some("0" | "false" | "no") => {}
            _ => {
                if HARNESSES.iter().any(|name| env(name).is_some()) {
                    return Mode::Agent;
                }
            }
        }
        if env("TERM").as_deref() == Some("dumb") {
            return Mode::Plain;
        }
        if env("FORCE_COLOR").is_some_and(|v| v != "0") {
            return Mode::Rich;
        }
        if tty {
            Mode::Rich
        } else {
            Mode::Plain
        }
    }

    /// A colour code, or nothing when colours are off.
    pub fn c(&self, code: &'static str) -> &'static str {
        if self.color {
            code
        } else {
            ""
        }
    }

    /// The Unicode symbol, or its ASCII stand-in.
    pub fn g(&self, unicode: &'static str, ascii: &'static str) -> &'static str {
        if self.utf {
            unicode
        } else {
            ascii
        }
    }
}

/// Counters that belong to a run, not to a call.
#[derive(Default)]
pub(crate) struct State {
    pub step_i: u32,
    pub step_n: u32,
}

/// A captured run: its own mode, environment, input and output.
pub(crate) struct Scope {
    pub theme: Theme,
    pub env: HashMap<String, String>,
    pub out: String,
    pub err: String,
    pub input: VecDeque<u8>,
    pub state: State,
}

impl Scope {
    pub fn new(mode: Mode, env: HashMap<String, String>, input: &str) -> Scope {
        let lookup = |name: &str| env.get(name).filter(|v| !v.is_empty()).cloned();
        let theme = Theme::detect(&lookup, false, Some(mode), None);
        Scope {
            theme,
            env,
            out: String::new(),
            err: String::new(),
            input: input.bytes().collect(),
            state: State::default(),
        }
    }
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

pub(crate) fn enter_scope(scope: Scope) {
    SCOPE.with(|s| *s.borrow_mut() = Some(scope));
}

pub(crate) fn leave_scope() -> Option<Scope> {
    SCOPE.with(|s| s.borrow_mut().take())
}

pub(crate) fn in_scope() -> bool {
    SCOPE.with(|s| s.borrow().is_some())
}

struct Forced {
    mode: Option<Mode>,
    color: Option<bool>,
}

static FORCED: RwLock<Forced> = RwLock::new(Forced { mode: None, color: None });
static THEME: RwLock<Option<Theme>> = RwLock::new(None);
static STATE: Mutex<State> = Mutex::new(State { step_i: 0, step_n: 0 });

fn real_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// An environment variable, if it is set to something.
pub(crate) fn env(name: &str) -> Option<String> {
    let scoped =
        SCOPE.with(|s| s.borrow().as_ref().map(|scope| scope.env.get(name).filter(|v| !v.is_empty()).cloned()));
    match scoped {
        Some(value) => value,
        None => real_env(name),
    }
}

pub(crate) fn flag(name: &str) -> bool {
    env(name).is_some()
}

pub(crate) fn theme() -> Theme {
    if let Some(theme) = SCOPE.with(|s| s.borrow().as_ref().map(|scope| scope.theme)) {
        return theme;
    }
    if let Some(theme) = *THEME.read().unwrap_or_else(|e| e.into_inner()) {
        return theme;
    }
    let forced = FORCED.read().unwrap_or_else(|e| e.into_inner());
    let mut tty = io::stdout().is_terminal() && io::stderr().is_terminal();
    if cfg!(windows) {
        // Without a Windows API call we cannot switch the console to ANSI ourselves.
        tty = tty && (real_env("WT_SESSION").is_some() || real_env("TERM").is_some());
    }
    let theme = Theme::detect(&real_env, tty, forced.mode, forced.color);
    *THEME.write().unwrap_or_else(|e| e.into_inner()) = Some(theme);
    theme
}

fn force(change: impl Fn(&mut Forced)) {
    let in_scope = SCOPE.with(|s| {
        let mut slot = s.borrow_mut();
        let Some(scope) = slot.as_mut() else { return false };
        let mut forced = Forced { mode: Some(scope.theme.mode), color: None };
        change(&mut forced);
        let env = &scope.env;
        let lookup = |name: &str| env.get(name).filter(|v| !v.is_empty()).cloned();
        scope.theme = Theme::detect(&lookup, false, forced.mode, forced.color);
        true
    });
    if !in_scope {
        change(&mut FORCED.write().unwrap_or_else(|e| e.into_inner()));
        *THEME.write().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// The mode in use.
pub fn mode() -> Mode {
    theme().mode
}

/// True when the output is being read by an LLM agent.
pub fn is_agent() -> bool {
    theme().mode == Mode::Agent
}

/// Force a mode for the rest of the run, whatever the environment says.
pub fn set_mode(mode: Mode) {
    force(|f| f.mode = Some(mode));
}

/// Go back to choosing the mode from the environment.
pub fn auto_mode() {
    force(|f| f.mode = None);
}

/// Turn colours off (or back on) without changing the mode.
pub fn set_color(on: bool) {
    force(|f| f.color = Some(on));
}

/// The mode, if [`set_mode`] forced one. Child processes are told about it.
pub(crate) fn forced_mode() -> Option<Mode> {
    FORCED.read().unwrap_or_else(|e| e.into_inner()).mode
}

pub(crate) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut f = Some(f);
    let scoped =
        SCOPE.with(|s| s.borrow_mut().as_mut().map(|scope| (f.take().expect("called once"))(&mut scope.state)));
    match scoped {
        Some(result) => result,
        None => (f.take().expect("called once"))(&mut STATE.lock().unwrap_or_else(|e| e.into_inner())),
    }
}

/// Write content to standard output.
pub(crate) fn out(text: &str) {
    let captured = SCOPE.with(|s| s.borrow_mut().as_mut().map(|scope| scope.out.push_str(text)));
    if captured.is_none() {
        let mut stream = io::stdout().lock();
        let _ = stream.write_all(text.as_bytes());
        let _ = stream.flush();
    }
}

/// Write status to standard error.
pub(crate) fn err(text: &str) {
    let captured = SCOPE.with(|s| s.borrow_mut().as_mut().map(|scope| scope.err.push_str(text)));
    if captured.is_none() {
        let mut stream = io::stderr().lock();
        let _ = stream.write_all(text.as_bytes());
        let _ = stream.flush();
    }
}

pub(crate) fn stdin_tty() -> bool {
    !in_scope() && io::stdin().is_terminal()
}

pub(crate) fn stderr_tty() -> bool {
    !in_scope() && io::stderr().is_terminal()
}

/// The terminal width in columns.
pub(crate) fn width() -> usize {
    if let Some(columns) = env("COLUMNS").and_then(|c| c.parse::<usize>().ok()).filter(|c| *c > 0) {
        return columns;
    }
    if stdin_tty() && cfg!(unix) {
        let size = std::process::Command::new("stty")
            .arg("size")
            .stdin(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::null())
            .output();
        if let Ok(size) = size {
            let text = String::from_utf8_lossy(&size.stdout);
            if let Some(columns) = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()) {
                if columns > 0 {
                    return columns;
                }
            }
        }
    }
    80
}

fn chomp(mut line: String) -> String {
    if line.ends_with('\n') {
        line.pop();
        if cfg!(windows) && line.ends_with('\r') {
            line.pop();
        }
    }
    line
}

fn scoped_input<R>(f: impl FnOnce(&mut VecDeque<u8>) -> R) -> Option<R> {
    SCOPE.with(|s| s.borrow_mut().as_mut().map(|scope| f(&mut scope.input)))
}

/// One line of standard input, without its newline. `None` at the end of the input.
pub(crate) fn read_line() -> Option<String> {
    let scoped = scoped_input(|input| {
        if input.is_empty() {
            return None;
        }
        let mut bytes = Vec::new();
        while let Some(byte) = input.pop_front() {
            if byte == b'\n' {
                break;
            }
            bytes.push(byte);
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    });
    if let Some(line) = scoped {
        return line;
    }
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(chomp(line)),
    }
}

/// Like [`read_line`], but gives up after `wait`. Used for agents, which must never hang.
pub(crate) fn read_line_timeout(wait: Duration) -> Option<String> {
    if in_scope() {
        return read_line();
    }
    // One reader thread for the whole run. The rendezvous channel keeps it from reading more
    // than one line ahead of what was asked for.
    static LINES: OnceLock<Mutex<Receiver<Option<String>>>> = OnceLock::new();
    let lines = LINES.get_or_init(|| {
        let (tx, rx) = sync_channel(0);
        std::thread::spawn(move || loop {
            let mut line = String::new();
            let read = io::stdin().lock().read_line(&mut line);
            let item = match read {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(chomp(line)),
            };
            let done = item.is_none();
            if tx.send(item).is_err() || done {
                break;
            }
        });
        Mutex::new(rx)
    });
    lines.lock().unwrap_or_else(|e| e.into_inner()).recv_timeout(wait).ok().flatten()
}

/// What one attempt to read a byte gave.
pub(crate) enum Byte {
    Got(u8),
    /// Nothing yet: a raw terminal read timed out.
    Nothing,
    End,
}

pub(crate) fn read_byte() -> Byte {
    let scoped = scoped_input(|input| input.pop_front());
    if let Some(byte) = scoped {
        return byte.map_or(Byte::End, Byte::Got);
    }
    let mut buffer = [0u8; 1];
    match io::stdin().lock().read(&mut buffer) {
        Ok(1) => Byte::Got(buffer[0]),
        // A terminal in raw mode with a read timeout reports "nothing yet" as a zero-length read.
        Ok(_) if io::stdin().is_terminal() => Byte::Nothing,
        _ => Byte::End,
    }
}
