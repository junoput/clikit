//! Status lines. They go to standard error, so they never mix with a program's real output.

use std::fmt::Display;

use crate::ctx::{self, Mode, BLUE, BOLD, CYAN, DIM, GRAY, GREEN, RED, RESET, YELLOW};

#[derive(Clone, Copy)]
pub(crate) enum Level {
    Info,
    Ok,
    Warn,
    Error,
    Debug,
}

pub(crate) fn msg(level: Level, text: &str) {
    let t = ctx::theme();
    match level {
        Level::Info | Level::Ok if ctx::flag("CLI_QUIET") => return,
        Level::Debug if !ctx::flag("CLI_DEBUG") => return,
        _ => {}
    }
    let (name, tag, color, glyph) = match level {
        Level::Info => ("info", "INFO", BLUE, t.g("●", "i")),
        Level::Ok => ("ok", "OK", GREEN, t.g("✓", "+")),
        Level::Warn => ("warn", "WARN", YELLOW, t.g("▲", "!")),
        Level::Error => ("error", "ERROR", RED, t.g("✗", "x")),
        Level::Debug => ("debug", "DEBUG", GRAY, t.g("·", ".")),
    };
    let line = match t.mode {
        Mode::Agent if matches!(level, Level::Info) => format!("{text}\n"),
        Mode::Agent => format!("{name}: {text}\n"),
        Mode::Plain => format!("[{tag}] {text}\n"),
        Mode::Rich => format!("{}{glyph}{} {text}\n", t.c(color), t.c(RESET)),
    };
    ctx::err(&line);
}

/// Something worth knowing.
pub fn info(text: impl Display) {
    msg(Level::Info, &text.to_string());
}

/// Something worked.
pub fn ok(text: impl Display) {
    msg(Level::Ok, &text.to_string());
}

/// Something needs attention.
pub fn warn(text: impl Display) {
    msg(Level::Warn, &text.to_string());
}

/// Something failed.
pub fn error(text: impl Display) {
    msg(Level::Error, &text.to_string());
}

/// Shown only when `CLI_DEBUG` is set.
pub fn debug(text: impl Display) {
    msg(Level::Debug, &text.to_string());
}

/// Print an error and exit with `code`.
pub fn die(text: impl Display, code: i32) -> ! {
    msg(Level::Error, &text.to_string());
    std::process::exit(code);
}

/// A tip for people. Agents never see it.
pub fn hint(text: impl Display) {
    let t = ctx::theme();
    if t.mode == Mode::Agent || ctx::flag("CLI_QUIET") {
        return;
    }
    ctx::err(&format!("{}{text}{}\n", t.c(DIM), t.c(RESET)));
}

/// Start a numbered run of [`step`] lines: `[1/3]`, `[2/3]`, ...
pub fn steps(total: u32) {
    ctx::with_state(|state| {
        state.step_n = total;
        state.step_i = 0;
    });
}

/// The next step: `[2/5] Build` after [`steps`], otherwise `▸ Build`.
pub fn step(text: impl Display) {
    if ctx::flag("CLI_QUIET") {
        return;
    }
    let t = ctx::theme();
    let (i, n) = ctx::with_state(|state| {
        state.step_i += 1;
        (state.step_i, state.step_n)
    });
    let counter = if n > 0 { format!("[{i}/{n}] ") } else { String::new() };
    let line = match t.mode {
        Mode::Agent => format!("{counter}{text}\n"),
        Mode::Plain => format!("==> {counter}{text}\n"),
        Mode::Rich => {
            let lead = if counter.is_empty() { format!("{} ", t.g("▸", ">")) } else { counter };
            format!("{}{}{lead}{}{}{text}{}\n", t.c(BOLD), t.c(CYAN), t.c(RESET), t.c(BOLD), t.c(RESET))
        }
    };
    ctx::err(&line);
}
