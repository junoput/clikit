//! A one-line editor with Tab completion, used by `input`, by the interactive prompt, and by
//! programs that have a prompt of their own ([`Editor`]).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::ctx::{self, Theme, BOLD, CYAN, DIM, RESET};
use crate::keys::{Key, Keys};
use crate::text::{len, pad, truncate, width};

/// The candidates of a list that are on show at once.
const LIST: usize = 8;

/// Something the word being typed could become.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// What is shown, and what is compared with what was typed.
    pub text: String,
    /// A few words about it. When any candidate has some, the candidates are shown as a list,
    /// one per line with its words, instead of in a row.
    pub about: String,
    /// What goes on the line, when that is not `text` as it is. Without it, on a command line,
    /// `text` goes in, in double quotes if it holds a space or a quote.
    pub insert: Option<String>,
    /// Only part of a word: see [`Candidate::partial`].
    pub partial: bool,
}

impl Candidate {
    pub fn new(text: impl Into<String>, about: impl Into<String>) -> Candidate {
        Candidate { text: text.into(), about: about.into(), insert: None, partial: false }
    }

    /// This is only the beginning of a word: a folder with more inside, `key=`, `user@`. When
    /// it is taken, no space follows and a quote that was opened for it stays open, so the
    /// person goes on typing right behind it and the next Tab completes inside.
    pub fn partial(mut self) -> Candidate {
        self.partial = true;
        self
    }

    /// Put this on the line instead of the text that is shown.
    pub fn insert(mut self, text: impl Into<String>) -> Candidate {
        self.insert = Some(text.into());
        self
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

/// What a completer answers: the candidates, and optionally which part of the line they replace.
///
/// A `Vec<Candidate>` is a `Completion` too: the candidates then replace the word being typed,
/// and those that do not start with it are left out. A word is what a shell would take as one:
/// `"two words"` and `two\ words` are one word each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completion {
    /// Where, in the text before the cursor, the part that is replaced begins (a byte offset).
    /// When this is given, the completer has decided what fits: no candidate is left out.
    pub from: Option<usize>,
    pub candidates: Vec<Candidate>,
}

impl Completion {
    /// Candidates that replace the text from byte `from` up to the cursor, all of them.
    pub fn at(from: usize, candidates: Vec<Candidate>) -> Completion {
        Completion { from: Some(from), candidates }
    }
}

impl From<Vec<Candidate>> for Completion {
    fn from(candidates: Vec<Candidate>) -> Completion {
        Completion { from: None, candidates }
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

/// The word being typed at the end of `before`, the way a shell splits words: where it starts
/// (a byte offset) and what it says without its quotes and backslashes.
fn last_word(before: &str) -> (usize, String) {
    let (mut start, mut word, mut quote) = (None, String::new(), None);
    let mut chars = before.char_indices();
    while let Some((at, c)) = chars.next() {
        match (quote, c) {
            (None, ' ' | '\t') => {
                start = None;
                word.clear();
                continue;
            }
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None | Some('"'), '\\') => {
                if let Some((_, escaped)) = chars.next() {
                    word.push(escaped);
                }
            }
            _ => word.push(c),
        }
        start.get_or_insert(at);
    }
    (start.unwrap_or(before.len()), word)
}

/// `text` as one word of a command line: as it is, or in double quotes when it needs them.
fn quoted(text: &str) -> String {
    if !text.is_empty() && !text.contains([' ', '\t', '"', '\'', '\\']) {
        return text.to_string();
    }
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The candidates under the line: in a row, or, when they come with words about them, as a
/// list. Ends with the move back up to the line.
fn menu(t: Theme, candidates: &[Candidate], current: usize, max_width: isize) -> String {
    let ellipsis = format!("{}{}{}", t.c(DIM), t.g("…", "~"), t.c(RESET));
    if candidates.iter().all(|candidate| candidate.about.is_empty()) {
        let (mut row, mut used) = (String::new(), 0);
        for (i, candidate) in candidates.iter().enumerate() {
            let candidate = &candidate.text;
            let (i, width) = (i + 1, len(candidate) as isize + 2);
            if used + width > max_width {
                if i > current {
                    row.push_str(&ellipsis);
                    break;
                }
                row = format!("{ellipsis}  ");
                used = 3;
            }
            if i == current {
                row.push_str(&format!("{}{}{candidate}{}  ", t.c(BOLD), t.c(CYAN), t.c(RESET)));
            } else {
                row.push_str(&format!("{}{candidate}{}  ", t.c(DIM), t.c(RESET)));
            }
            used += width;
        }
        return format!("\n{row}\x1b[1A\r"); // below the line, then back up to it
    }
    // A list: as many as fit a short page, the one picked in the middle of it where it can be.
    let n = candidates.len();
    let top = (current - 1).saturating_sub(LIST / 2).min(n.saturating_sub(LIST));
    let shown = &candidates[top..(top + LIST).min(n)];
    let name = shown.iter().map(|candidate| width(&candidate.text)).max().unwrap_or(0);
    let room = (max_width.max(10) as usize).saturating_sub(name + 2);
    let mut lines = Vec::new();
    for (i, candidate) in shown.iter().enumerate() {
        let (text, about) = (pad(&candidate.text, name), truncate(&candidate.about, room));
        lines.push(if top + i + 1 == current {
            format!("{}{}{text}{}  {about}", t.c(BOLD), t.c(CYAN), t.c(RESET))
        } else {
            format!("{}{text}  {about}{}", t.c(DIM), t.c(RESET))
        });
    }
    if n > LIST {
        lines.push(format!("{}{}-{} of {n}{}", t.c(DIM), top + 1, top + shown.len(), t.c(RESET)));
    }
    let lines: Vec<&str> = lines.iter().map(|line| line.trim_end()).collect();
    format!("\n{}\x1b[{}A\r", lines.join("\n"), lines.len())
}

/// Ctrl-R: look through the history for a line that holds what is typed. `Ok(line)` is a line
/// to run, `Err(Some(line))` one to go on editing, `Err(None)` nothing.
fn search(t: Theme, keys: &mut Keys, history: &[String]) -> Result<String, Option<String>> {
    let (mut query, mut skip) = (String::new(), 0);
    loop {
        let hits = || history.iter().rev().filter(|line| line.contains(&query));
        let hit = hits().nth(skip).cloned();
        let found = hit.as_deref().unwrap_or("");
        ctx::err(&format!("\r\x1b[J{}(search){} {query}{}:{} {found}", t.c(DIM), t.c(RESET), t.c(DIM), t.c(RESET)));
        match keys.next() {
            Key::Search if hits().nth(skip + 1).is_some() => skip += 1,
            Key::Backspace => (_, skip) = (query.pop(), 0),
            Key::Space => (_, skip) = (query.push(' '), 0),
            Key::Char(character) if !character.is_control() => (_, skip) = (query.push(character), 0),
            Key::Enter => return hit.ok_or(None),
            Key::Interrupt | Key::Eof | Key::EndOfInput => return Err(None),
            Key::Esc | Key::Left | Key::Right | Key::Home | Key::End | Key::Tab | Key::Up | Key::Down => {
                return Err(hit);
            }
            _ => {}
        }
    }
}

/// Read one line. `complete` is given the text before the cursor and returns what could stand
/// in place of the word (or, for a value, the whole text) being typed. Tab and Shift-Tab step
/// forwards and backwards through the candidates that match, shown underneath.
pub(crate) fn edit(prompt: &str, kind: Kind, complete: &dyn Fn(&str) -> Completion, history: &[String]) -> Edited {
    let t = ctx::theme();
    let max_width = ctx::width() as isize - 2;
    // What is left of the terminal's width for the line, the cursor included.
    let room = ((ctx::width() as isize - 1 - width(prompt) as isize).max(10)) as usize;
    let (mut left, mut right) = (String::new(), String::new());
    let (mut original, mut draft) = (String::new(), String::new());
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut current = 0; // 1-based position among the candidates; 0 when not stepping through them
    let mut word_at = 0; // where in `left` the text that the candidates replace begins
    let mut recalled = 0; // how far back in the history
    let mut scroll = 0; // the first character on show of a line too long for the terminal

    // On a command line Ctrl-C clears what was typed; in a question it ends the program.
    let mut keys = if kind == Kind::Line { Keys::open_with_interrupts() } else { Keys::open() };

    loop {
        let under =
            if current > 0 && candidates.len() > 1 { menu(t, &candidates, current, max_width) } else { String::new() };
        let (typed, cursor) = (len(&left) + len(&right), len(&left));
        let mut frame = format!("\r\x1b[J{under}{prompt}");
        if typed < room {
            scroll = 0;
            frame.push_str(&format!("{left}{right}"));
            if !right.is_empty() {
                frame.push_str(&format!("\x1b[{}D", len(&right)));
            }
        } else {
            // Longer than the terminal: show the part around the cursor, with a mark at each
            // end where there is more.
            scroll = scroll.min(cursor).max((cursor + 1).saturating_sub(room));
            let end = (scroll + room).min(typed);
            let mut shown: Vec<char> = left.chars().chain(right.chars()).skip(scroll).take(end - scroll).collect();
            let mark = t.g("…", "~").chars().next().unwrap_or('~');
            if scroll > 0 {
                shown[0] = mark;
            }
            if end < typed {
                *shown.last_mut().expect("a line on show") = mark;
            }
            frame.push_str(&shown.iter().collect::<String>());
            if end > cursor {
                frame.push_str(&format!("\x1b[{}D", end - cursor));
            }
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
                left.truncate(word_at);
                left.push_str(&original);
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
                    let completion = complete(&left);
                    let given = completion.from.filter(|from| kind == Kind::Line && left.is_char_boundary(*from));
                    let (from, stem) = match (kind, given) {
                        (Kind::Value, _) => (0, left.clone()),
                        (Kind::Line, Some(from)) => (from, String::new()),
                        (Kind::Line, None) => last_word(&left),
                    };
                    candidates = completion
                        .candidates
                        .into_iter()
                        .filter(|c| !c.text.is_empty() && (given.is_some() || c.text.starts_with(&stem)))
                        .collect();
                    if candidates.is_empty() {
                        continue;
                    }
                    (word_at, original) = (from, left[from..].to_string());
                    current = if key == Key::Tab { 1 } else { candidates.len() };
                } else if key == Key::Tab {
                    current = current % candidates.len() + 1;
                } else {
                    current = (current + candidates.len() - 2) % candidates.len() + 1;
                }
                let picked = &candidates[current - 1];
                left.truncate(word_at);
                let mut word = match (&picked.insert, kind) {
                    (Some(insert), _) => insert.clone(),
                    (None, Kind::Line) => quoted(&picked.text),
                    (None, Kind::Value) => picked.text.clone(),
                };
                if picked.partial && picked.insert.is_none() && word.starts_with('"') {
                    word.pop(); // the quote stays open: there is more of this word to come
                }
                left.push_str(&word);
                if candidates.len() == 1 {
                    // only one: take it
                    if kind == Kind::Line && !picked.partial {
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
            Key::WordLeft => {
                // Over the spaces before the cursor, then over the word before them.
                let word = left.trim_end_matches(' ').rfind(' ').map_or(0, |space| space + 1);
                right.insert_str(0, &left[word..]);
                left.truncate(word);
            }
            Key::WordRight => {
                let spaces = right.len() - right.trim_start_matches(' ').len();
                let word = right[spaces..].find(' ').map_or(right.len(), |space| spaces + space);
                left.push_str(&right[..word]);
                right.drain(..word);
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
            Key::Search if kind == Kind::Line && !history.is_empty() => match search(t, &mut keys, history) {
                Ok(line) => {
                    (left, right) = (line, String::new());
                    break;
                }
                Err(Some(line)) => (left, right) = (line, String::new()),
                Err(None) => {}
            },
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

type Completer<'a> = Box<dyn Fn(&str) -> Completion + 'a>;

/// A line editor for a program with a prompt of its own: a shell, a console, a REPL.
///
/// At a terminal the line can be edited (see the keys in `SPEC.md`; also Alt-B and Alt-F or
/// Ctrl-Left and Ctrl-Right to move by words, and Ctrl-R to search the history), Tab and
/// Shift-Tab step through what the completer offers, and Up and Down walk the history. A line
/// longer than the terminal scrolls sideways. Without a terminal it reads a line of standard
/// input.
///
/// ```no_run
/// use clikit::{Candidate, Editor, Read};
///
/// let mut editor = Editor::new()
///     .history_file(".myshell_history")
///     .completer(|before| match before.contains(' ') {
///         false => vec![Candidate::new("open", "Open a file"), Candidate::new("close", "Close it")],
///         true => vec!["notes.txt".into(), "My Documents".into()], // goes in as "My Documents"
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
        Editor { history: Vec::new(), file: None, complete: Box::new(|_| Completion::default()) }
    }

    /// What Tab offers. The function gets the text before the cursor and returns candidates:
    /// a `Vec<Candidate>` for the word being typed (see [`Completion`]), or a [`Completion`]
    /// that says itself which part of the line is replaced.
    pub fn completer<C: Into<Completion>>(mut self, complete: impl Fn(&str) -> C + 'a) -> Editor<'a> {
        self.complete = Box::new(move |before| complete(before).into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_being_typed_is_a_shell_word() {
        assert_eq!(last_word("open rep"), (5, "rep".to_string()));
        assert_eq!(last_word("open \"Annual re"), (5, "Annual re".to_string()));
        assert_eq!(last_word("get Library/App\\ Sup"), (4, "Library/App Sup".to_string()));
        assert_eq!(last_word("open "), (5, String::new()));
        assert_eq!(last_word("a 'b c' d"), (8, "d".to_string()));
        assert_eq!(last_word(""), (0, String::new()));
    }

    #[test]
    fn a_candidate_goes_in_as_one_word() {
        assert_eq!(quoted("list"), "list");
        assert_eq!(quoted("Annual report 2026"), "\"Annual report 2026\"");
        assert_eq!(quoted("say \"hi\""), "\"say \\\"hi\\\"\"");
        // What goes in comes out again as the one word it was.
        assert_eq!(crate::split_words(&quoted("a \\ \"b\" 'c'")).unwrap(), ["a \\ \"b\" 'c'"]);
    }
}
