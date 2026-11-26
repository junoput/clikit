//! A one-line editor with Tab completion, used by `input`, by the interactive prompt, and by
//! programs that have a prompt of their own ([`Editor`]).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::ctx::{self, BOLD, CYAN, DIM, RESET};
use crate::keys::{Key, Keys};
use crate::text::{len, truncate};

/// Something the word being typed could become, and optionally a few words about it, which
/// are shown while it is the one picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub about: String,
}

impl Candidate {
    pub fn new(text: impl Into<String>, about: impl Into<String>) -> Candidate {
        Candidate { text: text.into(), about: about.into() }
    }
}

impl From<&str> for Candidate {
    fn from(text: &str) -> Candidate {
        Candidate::new(text, "")
    }
}

impl From<String> for Candidate {
    fn from(text: String) -> Candidate {
        Candidate::new(text, "")
    }
}

impl From<(&str, &str)> for Candidate {
    fn from((text, about): (&str, &str)) -> Candidate {
        Candidate::new(text, about)
    }
}

/// What the text is, which decides what Tab, the arrow keys and Escape do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A command line: Tab completes the word under the cursor, up and down walk the history.
    Line,
    /// One answer: Tab completes the whole text, Escape cancels.
    Value,
}

pub(crate) enum Edited {
    Text(String),
    /// The input ended, or Ctrl-D on an empty line.
    End,
    /// Escape.
    Cancelled,
    /// Ctrl-C on an empty command line.
    Interrupted,
}

fn pop_first(text: &mut String) -> Option<char> {
    let first = text.chars().next()?;
    text.drain(..first.len_utf8());
    Some(first)
}

/// Read one line. `complete` is given the text before the cursor and returns what could stand
/// in place of the word (or, for a value, the whole text) being typed. Tab and Shift-Tab step
/// forwards and backwards through the candidates that match, shown on a row underneath.
pub(crate) fn edit(prompt: &str, kind: Kind, complete: &dyn Fn(&str) -> Vec<Candidate>, history: &[String]) -> Edited {
    let t = ctx::theme();
    let max_width = ctx::width() as isize - 2;
    let ellipsis = format!("{}{}{}", t.c(DIM), t.g("…", "~"), t.c(RESET));
    let (mut left, mut right) = (String::new(), String::new());
    let (mut stem, mut word, mut draft) = (String::new(), String::new(), String::new());
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut current = 0; // 1-based position among the candidates; 0 when not stepping through them
    let mut recalled = 0; // how far back in the history
                          // On a command line Ctrl-C clears what was typed; in a question it ends the program.
    let mut keys = if kind == Kind::Line { Keys::open_with_interrupts() } else { Keys::open() };

    loop {
        let mut menu = String::new();
        if current > 0 && candidates.len() > 1 {
            let mut used = 0;
            for (i, candidate) in candidates.iter().enumerate() {
                let candidate = &candidate.text;
                let (i, width) = (i + 1, len(candidate) as isize + 2);
                if used + width > max_width {
                    if i > current {
                        menu.push_str(&ellipsis);
                        break;
                    }
                    menu = format!("{ellipsis}  ");
                    used = 3;
                }
                if i == current {
                    menu.push_str(&format!("{}{}{candidate}{}  ", t.c(BOLD), t.c(CYAN), t.c(RESET)));
                } else {
                    menu.push_str(&format!("{}{candidate}{}  ", t.c(DIM), t.c(RESET)));
                }
                used += width;
            }
            // What the candidate picked is, if it says and there is room left.
            let about = &candidates[current - 1].about;
            if !about.is_empty() && max_width - used >= 4 {
                menu.push_str(&format!("{}{}{}", t.c(DIM), truncate(about, (max_width - used) as usize), t.c(RESET)));
            }
            menu = format!("\n{menu}\x1b[1A\r"); // below the line, then back up to it
        }
        let mut frame = format!("\r\x1b[J{menu}{prompt}{left}{right}");
        if !right.is_empty() {
            frame.push_str(&format!("\x1b[{}D", len(&right)));
        }
        ctx::err(&frame);

        let key = keys.next();
        if key == Key::EndOfInput {
            if !left.is_empty() || !right.is_empty() {
                break;
            }
            drop(keys);
            ctx::err("\n");
            return Edited::End;
        }
        match key {
            Key::Tab | Key::BackTab => {}
            Key::Esc if current > 0 => {
                left.truncate(left.len() - word.len());
                left.push_str(&stem);
                current = 0;
                candidates.clear();
                continue;
            }
            _ => {
                current = 0;
                candidates.clear();
            }
        }
        match key {
            Key::Enter => break,
            Key::Tab | Key::BackTab => {
                if current == 0 {
                    stem = match kind {
                        Kind::Line => left.rsplit(' ').next().unwrap_or_default().to_string(),
                        Kind::Value => left.clone(),
                    };
                    word = stem.clone();
                    candidates = complete(&left)
                        .into_iter()
                        .filter(|c| !c.text.is_empty() && c.text.starts_with(&stem))
                        .collect();
                    if candidates.is_empty() {
                        continue;
                    }
                    current = if key == Key::Tab { 1 } else { candidates.len() };
                } else if key == Key::Tab {
                    current = current % candidates.len() + 1;
                } else {
                    current = (current + candidates.len() - 2) % candidates.len() + 1;
                }
                left.truncate(left.len() - word.len());
                word = candidates[current - 1].text.clone();
                left.push_str(&word);
                if candidates.len() == 1 {
                    // only one: take it
                    if kind == Kind::Line {
                        left.push(' ');
                    }
                    current = 0;
                    candidates.clear();
                }
            }
            Key::Esc => {
                if kind == Kind::Value {
                    drop(keys);
                    ctx::err(&format!("\r\x1b[J{prompt}{left}{right}\n"));
                    return Edited::Cancelled;
                }
            }
            Key::Backspace => {
                left.pop();
            }
            Key::Delete => {
                pop_first(&mut right);
            }
            Key::Left => {
                if let Some(character) = left.pop() {
                    right.insert(0, character);
                }
            }
            Key::Right => {
                if let Some(character) = pop_first(&mut right) {
                    left.push(character);
                }
            }
            Key::Home => {
                right.insert_str(0, &left);
                left.clear();
            }
            Key::End => {
                left.push_str(&right);
                right.clear();
            }
            Key::KillLeft => left.clear(),
            Key::KillRight => right.clear(),
            Key::KillWord => {
                left.truncate(left.trim_end_matches(' ').len());
                left.truncate(left.rfind(' ').map_or(0, |space| space + 1));
            }
            Key::Up | Key::Down => {
                if kind != Kind::Line {
                    continue;
                }
                if key == Key::Up && recalled < history.len() {
                    if recalled == 0 {
                        draft = format!("{left}{right}");
                    }
                    recalled += 1;
                } else if key == Key::Down && recalled > 0 {
                    recalled -= 1;
                } else {
                    continue;
                }
                right.clear();
                left = if recalled > 0 { history[history.len() - recalled].clone() } else { draft.clone() };
            }
            Key::Eof => {
                if left.is_empty() && right.is_empty() {
                    drop(keys);
                    ctx::err("\n");
                    return Edited::End;
                }
            }
            // Ctrl-C: a line with something on it is dropped; an empty one says "enough".
            Key::Interrupt if left.is_empty() && right.is_empty() => {
                drop(keys);
                ctx::err(&format!("\r\x1b[J{prompt}^C\n"));
                return Edited::Interrupted;
            }
            Key::Interrupt => {
                ctx::err(&format!("\r\x1b[J{prompt}{left}{right}^C\n"));
                (left, right, recalled) = (String::new(), String::new(), 0);
            }
            Key::Space => left.push(' '),
            Key::Char(character) if !character.is_control() => left.push(character),
            _ => {}
        }
    }
    drop(keys);
    let line = format!("{left}{right}");
    ctx::err(&format!("\r\x1b[J{prompt}{line}\n"));
    Edited::Text(line)
}

/// What [`Editor::read`] came back with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// A line, as it was typed.
    Line(String),
    /// Ctrl-C on an empty line. (On a line with text, Ctrl-C drops the text and reading goes on.)
    Interrupted,
    /// The input ended, or Ctrl-D on an empty line.
    End,
}

type Completer<'a> = Box<dyn Fn(&str) -> Vec<Candidate> + 'a>;

/// A line editor for a program with a prompt of its own: a shell, a console, a REPL.
///
/// At a terminal the line can be edited (see the keys in `SPEC.md`), Tab and Shift-Tab step
/// through what the completer offers, and Up and Down walk the history. Without a terminal
/// it reads a line of standard input.
///
/// ```no_run
/// use clikit::{Candidate, Editor, Read};
///
/// let mut editor = Editor::new()
///     .history_file(".myshell_history")
///     .completer(|before| match before.rsplit(' ').nth(1) {
///         None => vec![Candidate::new("open", "Open a file"), Candidate::new("close", "Close it")],
///         Some(_) => vec!["notes.txt".into(), "todo.txt".into()],
///     });
/// while let Read::Line(line) = editor.read("myshell> ") {
///     println!("you typed: {line}");
/// }
/// ```
pub struct Editor<'a> {
    history: Vec<String>,
    file: Option<PathBuf>,
    complete: Completer<'a>,
}

impl Default for Editor<'_> {
    fn default() -> Self {
        Editor::new()
    }
}

impl<'a> Editor<'a> {
    pub fn new() -> Editor<'a> {
        Editor { history: Vec::new(), file: None, complete: Box::new(|_| Vec::new()) }
    }

    /// What Tab offers. The function gets the text before the cursor and returns what the
    /// word being typed (everything after the last space) could become; candidates that do not
    /// start with what was typed are left out.
    pub fn completer(mut self, complete: impl Fn(&str) -> Vec<Candidate> + 'a) -> Editor<'a> {
        self.complete = Box::new(complete);
        self
    }

    /// Keep the history in a file: its lines are read now, and every line that is entered is
    /// added to it. A file that cannot be read or written is no error: the history then lasts
    /// as long as the program.
    pub fn history_file(mut self, path: impl Into<PathBuf>) -> Editor<'a> {
        let path = path.into();
        if let Ok(text) = std::fs::read_to_string(&path) {
            self.history.extend(text.lines().filter(|line| !line.trim().is_empty()).map(String::from));
        }
        self.file = Some(path);
        self
    }

    /// The lines entered so far, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Add a line to the history, unless it is blank or the same as the one before.
    pub fn remember(&mut self, line: &str) {
        if line.trim().is_empty() || self.history.last().is_some_and(|last| last == line) {
            return;
        }
        self.history.push(line.to_string());
        if let Some(path) = &self.file {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    /// Read one line, with `prompt` in front of it.
    pub fn read(&mut self, prompt: &str) -> Read {
        if !crate::prompt::keys_available() {
            return ctx::read_line().map_or(Read::End, Read::Line);
        }
        match edit(prompt, Kind::Line, &self.complete, &self.history) {
            Edited::Text(line) => {
                self.remember(&line);
                Read::Line(line)
            }
            Edited::Interrupted => Read::Interrupted,
            Edited::End | Edited::Cancelled => Read::End,
        }
    }
}
