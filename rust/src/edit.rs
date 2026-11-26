//! A one-line editor with Tab completion, used by `input` and by the interactive prompt.

use crate::ctx::{self, BOLD, CYAN, DIM, RESET};
use crate::keys::{Key, Keys};
use crate::text::len;

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
}

fn pop_first(text: &mut String) -> Option<char> {
    let first = text.chars().next()?;
    text.drain(..first.len_utf8());
    Some(first)
}

/// Read one line. `complete` is given the text before the cursor and returns what could stand
/// in place of the word (or, for a value, the whole text) being typed. Tab and Shift-Tab step
/// forwards and backwards through the candidates that match, shown on a row underneath.
pub(crate) fn edit(prompt: &str, kind: Kind, complete: &dyn Fn(&str) -> Vec<String>, history: &[String]) -> Edited {
    let t = ctx::theme();
    let max_width = ctx::width() as isize - 2;
    let ellipsis = format!("{}{}{}", t.c(DIM), t.g("…", "~"), t.c(RESET));
    let (mut left, mut right) = (String::new(), String::new());
    let (mut stem, mut word, mut draft) = (String::new(), String::new(), String::new());
    let mut candidates: Vec<String> = Vec::new();
    let mut current = 0; // 1-based position among the candidates; 0 when not stepping through them
    let mut recalled = 0; // how far back in the history
    let mut keys = Keys::open();

    loop {
        let mut menu = String::new();
        if current > 0 && candidates.len() > 1 {
            let mut used = 0;
            for (i, candidate) in candidates.iter().enumerate() {
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
                    candidates =
                        complete(&left).into_iter().filter(|c| !c.is_empty() && c.starts_with(&stem)).collect();
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
                word = candidates[current - 1].clone();
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
