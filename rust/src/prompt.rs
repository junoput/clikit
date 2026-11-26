//! Questions. The prompt is drawn on standard error and the answer is returned, so a program's
//! standard output stays clean.
//!
//! At a terminal, [`select`] and [`multiselect`] are arrow-key menus (Tab and Shift-Tab move
//! too), and [`input_with`] completes from a list of suggestions. Escape cancels a question,
//! which [`wizard`] uses to step back to the one before. Without a terminal they print
//! the options and read a line, which may be a number or the option's text. In agent
//! mode nothing ever blocks: an answer is taken from standard input if one arrives within a
//! second, then the default is used, then the prompt fails with a message that says what to
//! send.

use std::fmt;
use std::time::Duration;

use crate::ctx::{self, Mode, BOLD, CYAN, DIM, GREEN, RESET};
use crate::edit::{edit, Edited, Kind};
use crate::keys::{Key, Keys};
use crate::msg::{msg, Level};
use crate::text::truncate;

/// Why a prompt returned no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptError {
    /// Nobody answered and there was no default.
    NoAnswer,
    /// The person pressed Escape. In a [`wizard`] this steps back one question.
    Cancelled,
    /// The answer was not one of the options.
    Invalid(String),
    /// There was nothing to choose from.
    NoOptions,
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromptError::NoAnswer => f.write_str("no answer was given"),
            PromptError::Cancelled => f.write_str("cancelled"),
            PromptError::Invalid(answer) => write!(f, "invalid choice '{answer}'"),
            PromptError::NoOptions => f.write_str("no options to choose from"),
        }
    }
}

impl std::error::Error for PromptError {}

/// Can we run a key-driven prompt?
pub(crate) fn keys_available() -> bool {
    match ctx::env("CLI_INTERACTIVE").as_deref() {
        Some("0" | "false" | "no") => return false,
        Some("1" | "true" | "yes") => return true,
        _ => {}
    }
    cfg!(unix) && ctx::theme().mode == Mode::Rich && ctx::stdin_tty() && ctx::stderr_tty()
}

/// One line of standard input. Agent mode never blocks: no terminal reads, one second at most.
pub(crate) fn read_answer() -> Option<String> {
    if ctx::theme().mode == Mode::Agent {
        if ctx::stdin_tty() {
            return None;
        }
        ctx::read_line_timeout(Duration::from_secs(1))
    } else {
        ctx::read_line()
    }
}

/// A yes/no question. `default` is what Enter (or no input at all) means.
///
/// `CLI_YES=1` answers yes without asking. Escape gives [`PromptError::Cancelled`].
///
/// ```no_run
/// if clikit::confirm("Deploy to production?", Some(false)).unwrap_or(false) {
///     // ...
/// }
/// ```
pub fn confirm(question: &str, default: Option<bool>) -> Result<bool, PromptError> {
    let t = ctx::theme();
    let hint = match default {
        Some(true) => "[Y/n]",
        Some(false) => "[y/N]",
        None => "[y/n]",
    };
    if ctx::flag("CLI_YES") {
        return Ok(true);
    }
    if keys_available() {
        ctx::err(&format!("{}?{} {question} {}{hint}{} ", t.c(CYAN), t.c(RESET), t.c(DIM), t.c(RESET)));
        let mut keys = Keys::open();
        let mut cancelled = false;
        let answer = loop {
            match keys.next() {
                Key::EndOfInput => break default.unwrap_or(false),
                Key::Char('y' | 'Y') => break true,
                Key::Char('n' | 'N') => break false,
                Key::Esc => {
                    cancelled = true;
                    break false;
                }
                Key::Enter => {
                    if let Some(answer) = default {
                        break answer;
                    }
                }
                _ => {}
            }
        };
        drop(keys);
        ctx::err(if answer { "yes\n" } else { "no\n" });
        return if cancelled { Err(PromptError::Cancelled) } else { Ok(answer) };
    }
    if t.mode != Mode::Agent {
        ctx::err(&format!("{question} {hint} "));
    }
    let answer = match read_answer().unwrap_or_default().as_str() {
        "y" | "Y" | "yes" | "Yes" | "YES" => Some(true),
        "n" | "N" | "no" | "No" | "NO" => Some(false),
        "" => default,
        _ => None,
    };
    let letter = match answer {
        Some(true) => "y",
        Some(false) => "n",
        None => "",
    };
    if t.mode == Mode::Agent {
        if answer.is_some() {
            ctx::err(&format!("{question} {letter}\n"));
        }
    } else if !ctx::stdin_tty() {
        ctx::err(&format!("{letter}\n"));
    }
    answer.ok_or_else(|| {
        msg(Level::Error, &format!("input required: {question} [y/n]. Pipe y or n on stdin, or set CLI_YES=1."));
        PromptError::NoAnswer
    })
}

/// A line of text.
pub fn input(question: &str, default: Option<&str>) -> Result<String, PromptError> {
    input_with::<&str>(question, default, &[])
}

/// A line of text, with suggestions. At a terminal, Tab and Shift-Tab step through the
/// suggestions that start with what was typed. Anything may still be typed.
///
/// ```no_run
/// let branch = clikit::input_with("Branch", Some("main"), &["main", "dev", "release"])?;
/// # Ok::<(), clikit::PromptError>(())
/// ```
pub fn input_with<S: AsRef<str>>(
    question: &str,
    default: Option<&str>,
    suggestions: &[S],
) -> Result<String, PromptError> {
    let t = ctx::theme();
    let default = default.unwrap_or("");
    let (answer, answered);
    if keys_available() {
        let shown = if default.is_empty() { String::new() } else { format!(" {}({default}){}", t.c(DIM), t.c(RESET)) };
        let prompt = format!("{}?{} {question}{shown} ", t.c(CYAN), t.c(RESET));
        let complete = |_: &str| suggestions.iter().map(|s| s.as_ref().into()).collect::<Vec<_>>().into();
        match edit(&prompt, Kind::Value, &complete, &[]) {
            Edited::Cancelled => return Err(PromptError::Cancelled),
            Edited::Text(text) => (answer, answered) = (text, true),
            Edited::End | Edited::Interrupted => (answer, answered) = (String::new(), false),
        }
    } else {
        if t.mode != Mode::Agent {
            let shown = if default.is_empty() { String::new() } else { format!(" [{default}]") };
            ctx::err(&format!("{question}{shown}: "));
        }
        let line = read_answer();
        answered = line.is_some();
        answer = line.unwrap_or_default();
        if t.mode != Mode::Agent && !ctx::stdin_tty() {
            ctx::err(&format!("{}\n", if answer.is_empty() { default } else { &answer }));
        }
    }
    let answer = if answer.is_empty() { default.to_string() } else { answer };
    if answer.is_empty() && !answered {
        let names: Vec<&str> = suggestions.iter().map(|s| s.as_ref()).collect();
        let hint = if names.is_empty() { String::new() } else { format!(" (suggestions: {})", names.join(" | ")) };
        msg(Level::Error, &format!("input required: {question}{hint}. Pipe the answer on stdin."));
        return Err(PromptError::NoAnswer);
    }
    if t.mode == Mode::Agent {
        ctx::err(&format!("{question}: {answer}\n"));
    }
    Ok(answer)
}

/// A line of text that is not shown while it is typed.
pub fn password(question: &str) -> Result<String, PromptError> {
    let t = ctx::theme();
    // At a terminal the typing is hidden whatever the mode: `tool export | tar x` is plain,
    // because its output is a pipe, and still has a person typing the password.
    let typed = ctx::stdin_tty() && t.mode != Mode::Agent;
    if !keys_available() && !typed {
        return read_answer().ok_or_else(|| {
            msg(Level::Error, &format!("input required: {question}. Pipe the answer on stdin."));
            PromptError::NoAnswer
        });
    }
    // Echo goes off before the question shows: what is typed the moment it appears stays hidden.
    let mut keys = Keys::open();
    ctx::err(&format!("{}?{} {question} ", t.c(CYAN), t.c(RESET)));
    let answer = if keys.raw() {
        let mut typed = String::new();
        loop {
            match keys.next() {
                Key::Enter | Key::EndOfInput | Key::Eof => break,
                Key::Space => typed.push(' '),
                Key::Backspace => {
                    typed.pop();
                }
                Key::KillLeft => typed.clear(),
                Key::Char(character) => typed.push(character),
                _ => {}
            }
        }
        typed
    } else {
        ctx::read_line().unwrap_or_default()
    };
    drop(keys);
    ctx::err("\n");
    Ok(answer)
}

/// Wait for a key. Does nothing without a terminal.
pub fn pause(text: Option<&str>) {
    if !keys_available() {
        return;
    }
    let t = ctx::theme();
    ctx::err(&format!("{}{}{}", t.c(DIM), text.unwrap_or("Press any key to continue"), t.c(RESET)));
    let mut keys = Keys::open();
    keys.next();
    drop(keys);
    ctx::err("\r\x1b[K");
}

/// Pick one option. `default` is where the cursor starts, and the answer when nobody answers.
///
/// ```no_run
/// let target = clikit::select("Deploy to", &["staging", "production"], Some("staging"))?;
/// # Ok::<(), clikit::PromptError>(())
/// ```
pub fn select<S: AsRef<str>>(question: &str, options: &[S], default: Option<&str>) -> Result<String, PromptError> {
    let defaults: Vec<&str> = default.into_iter().collect();
    pick(false, question, &defaults, options).map(|mut chosen| chosen.pop().unwrap_or_default())
}

/// Pick any number of options. `defaults` start ticked.
pub fn multiselect<S: AsRef<str>>(
    question: &str,
    options: &[S],
    defaults: &[&str],
) -> Result<Vec<String>, PromptError> {
    pick(true, question, defaults, options)
}

fn pick<S: AsRef<str>>(multi: bool, title: &str, defaults: &[&str], options: &[S]) -> Result<Vec<String>, PromptError> {
    let t = ctx::theme();
    let items: Vec<&str> = options.iter().map(|option| option.as_ref()).collect();
    let n = items.len();
    if n == 0 {
        msg(Level::Error, &format!("{title}: no options to choose from"));
        return Err(PromptError::NoOptions);
    }
    if !keys_available() {
        return pick_line(multi, title, defaults, &items);
    }

    let mut marks = vec![false; n];
    let mut cur = 0;
    for (i, item) in items.iter().enumerate() {
        if defaults.contains(item) {
            if multi {
                marks[i] = true;
            } else {
                cur = i;
            }
        }
    }
    let page = ctx::env("CLI_PAGE").and_then(|p| p.parse::<usize>().ok()).unwrap_or(10).clamp(1, n);
    let max_width = ctx::width().saturating_sub(8);
    let (cyan, bold, dim, green, reset) = (t.c(CYAN), t.c(BOLD), t.c(DIM), t.c(GREEN), t.c(RESET));
    let arrows = t.g("↑/↓", "up/down");
    let hint = if multi {
        format!("{arrows} move, space toggle, a all, enter confirm")
    } else {
        format!("{arrows} move, enter select")
    };
    ctx::err(&format!("{cyan}?{reset} {bold}{title}{reset}  {dim}{hint}{reset}\n"));

    let mut keys = Keys::open();
    let mut top = 0;
    let drawn = loop {
        if cur < top {
            top = cur;
        }
        if cur >= top + page {
            top = cur + 1 - page;
        }
        let mut frame = String::new();
        for (i, item) in items.iter().enumerate().skip(top).take(page) {
            let item = truncate(item, max_width);
            let mark = match (multi, marks[i]) {
                (false, _) => String::new(),
                (true, true) => format!("{green}{}{reset} ", t.g("◉", "[x]")),
                (true, false) => format!("{dim}{}{reset} ", t.g("○", "[ ]")),
            };
            if i == cur {
                frame.push_str(&format!("{cyan}{}{reset} {mark}{bold}{item}{reset}\x1b[K\n", t.g("❯", ">")));
            } else {
                frame.push_str(&format!("  {mark}{item}\x1b[K\n"));
            }
        }
        let mut drawn = page;
        if n > page {
            frame.push_str(&format!("  {dim}{}-{} of {n}{reset}\x1b[K\n", top + 1, top + page));
            drawn += 1;
        }
        ctx::err(&frame);
        match keys.next() {
            Key::Up | Key::Char('k') | Key::BackTab => cur = if cur == 0 { n - 1 } else { cur - 1 },
            Key::Down | Key::Char('j') | Key::Tab => cur = if cur + 1 == n { 0 } else { cur + 1 },
            Key::Space if multi => marks[cur] = !marks[cur],
            Key::Char('a') if multi => {
                let all = marks.iter().all(|mark| !mark);
                marks.iter_mut().for_each(|mark| *mark = all);
            }
            Key::Enter => break drawn,
            key @ (Key::Esc | Key::EndOfInput) => {
                drop(keys);
                ctx::err(&format!("\x1b[{}A\x1b[J{dim}{} {title} (cancelled){reset}\n", drawn + 1, t.g("✗", "x")));
                // Escape is its own error, so that a wizard can step back; running out of input is not.
                return Err(if key == Key::Esc { PromptError::Cancelled } else { PromptError::NoAnswer });
            }
            _ => {}
        }
        ctx::err(&format!("\x1b[{drawn}A"));
    };
    drop(keys);

    let chosen: Vec<String> = if multi {
        items.iter().zip(&marks).filter(|(_, mark)| **mark).map(|(item, _)| item.to_string()).collect()
    } else {
        vec![items[cur].to_string()]
    };
    let shown = if chosen.is_empty() { "(none)".to_string() } else { chosen.join(", ") };
    ctx::err(&format!(
        "\x1b[{}A\x1b[J{green}{}{reset} {title} {dim}{}{reset} {shown}\n",
        drawn + 1,
        t.g("✓", "+"),
        t.g("›", ":"),
    ));
    Ok(chosen)
}

/// The same question without a terminal: one line in, by number or by name.
fn pick_line(multi: bool, title: &str, defaults: &[&str], items: &[&str]) -> Result<Vec<String>, PromptError> {
    let agent = ctx::theme().mode == Mode::Agent;
    let options = items.join(" | ");
    if !agent {
        let mut text = format!("{title}\n");
        for (i, item) in items.iter().enumerate() {
            text.push_str(&format!("  {}) {item}\n", i + 1));
        }
        let shown = if defaults.is_empty() { String::new() } else { format!(" [{}]", defaults.join(", ")) };
        text.push_str(&format!("choice{shown}: "));
        ctx::err(&text);
    }
    let line = read_answer();
    let answered = line.is_some();
    let answer = line.unwrap_or_default();
    if !agent && !ctx::stdin_tty() {
        ctx::err(&format!("{}\n", if answer.is_empty() { defaults.join(", ") } else { answer.clone() }));
    }

    let mut chosen: Vec<String> = Vec::new();
    if answer.is_empty() {
        chosen = defaults.iter().map(|d| d.to_string()).collect();
        if chosen.is_empty() && (!multi || !answered) {
            msg(Level::Error, &format!("input required: {title} (options: {options}). Pipe the answer on stdin."));
            return Err(PromptError::NoAnswer);
        }
    } else {
        let parts: Vec<&str> = if multi {
            answer.strip_suffix(',').unwrap_or(&answer).split(',').collect()
        } else {
            vec![answer.as_str()]
        };
        for part in parts {
            let part = part.trim_matches(' ');
            let by_name = items.iter().find(|item| **item == part);
            let by_number =
                || part.parse::<usize>().ok().filter(|i| (1..=items.len()).contains(i)).map(|i| &items[i - 1]);
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            match by_name.or_else(|| if digits { by_number() } else { None }) {
                Some(item) => chosen.push(item.to_string()),
                None => {
                    msg(Level::Error, &format!("invalid choice '{part}' for: {title} (options: {options})"));
                    return Err(PromptError::Invalid(part.to_string()));
                }
            }
        }
    }
    if agent {
        ctx::err(&format!("{title}: {}\n", chosen.join(", ")));
    }
    Ok(chosen)
}

/// Ask questions in order; Escape steps back to the one before.
///
/// Each step asks one question and keeps the answer in `state`. A step that returns
/// [`PromptError::Cancelled`] (Escape) sends the wizard back one step, or ends it with that
/// error when it was the first. Any other error ends the wizard.
///
/// ```no_run
/// use clikit::PromptError;
///
/// #[derive(Default)]
/// struct Answers {
///     name: String,
///     target: String,
/// }
///
/// let mut answers = Answers::default();
/// clikit::wizard(
///     &mut answers,
///     &[
///         &|a: &mut Answers| {
///             a.name = clikit::input("Name", Some(&a.name))?;
///             Ok(())
///         },
///         &|a: &mut Answers| {
///             a.target = clikit::select("Deploy to", &["staging", "production"], Some(&a.target))?;
///             Ok(())
///         },
///     ],
/// )?;
/// # Ok::<(), PromptError>(())
/// ```
#[allow(clippy::type_complexity)]
pub fn wizard<S>(state: &mut S, steps: &[&dyn Fn(&mut S) -> Result<(), PromptError>]) -> Result<(), PromptError> {
    let mut i = 0;
    while i < steps.len() {
        match steps[i](state) {
            Ok(()) => i += 1,
            Err(PromptError::Cancelled) if i > 0 => i -= 1,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
