//! Reading single keys, from a terminal in raw mode or from piped input.

use std::process::{Command, Stdio};

use crate::ctx::{self, Byte};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Key {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
    Backspace,
    Tab,
    BackTab,
    Enter,
    Space,
    Esc,
    /// Ctrl-U, Ctrl-K, Ctrl-W.
    KillLeft,
    KillRight,
    KillWord,
    /// Ctrl-D.
    Eof,
    /// A key we have no use for, such as Page Up.
    Other,
    Char(char),
    /// There is nothing more to read.
    EndOfInput,
}

/// Reads keys. On a real terminal it switches to raw mode with `stty` and switches back when
/// dropped.
pub(crate) struct Keys {
    saved: Option<String>,
    unread: Option<u8>,
}

fn stty(args: &[&str]) {
    let _ = Command::new("stty").args(args).stdin(Stdio::inherit()).stderr(Stdio::null()).status();
}

impl Keys {
    pub fn open() -> Keys {
        if !ctx::stdin_tty() {
            return Keys { saved: None, unread: None };
        }
        let saved = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
        if saved.is_some() {
            // No line buffering, no echo, and Ctrl-C arrives as a byte so we can restore the
            // terminal before leaving. A read returns after 0.1s if nothing was typed.
            stty(&["-icanon", "-echo", "-isig", "min", "0", "time", "1"]);
        }
        Keys { saved, unread: None }
    }

    fn restore(&mut self) {
        if let Some(saved) = self.saved.take() {
            stty(&[&saved]);
        }
    }

    /// True on a real terminal.
    pub fn raw(&self) -> bool {
        self.saved.is_some()
    }

    /// The next byte. With `wait` it blocks until there is one; without, it gives up after
    /// a moment (on a terminal) and returns `None`. `None` also means the input has ended.
    fn byte(&mut self, wait: bool) -> Option<u8> {
        if let Some(byte) = self.unread.take() {
            return Some(byte);
        }
        loop {
            match ctx::read_byte() {
                Byte::Got(byte) => return Some(byte),
                Byte::Nothing if self.raw() && wait => continue,
                _ => return None,
            }
        }
    }

    pub fn next(&mut self) -> Key {
        let Some(first) = self.byte(true) else { return Key::EndOfInput };
        match first {
            0x03 if self.raw() => {
                self.restore();
                ctx::err("\n");
                std::process::exit(130);
            }
            0x1b => self.escape(),
            b'\n' | b'\r' => Key::Enter,
            b' ' => Key::Space,
            b'\t' => Key::Tab,
            0x7f | 0x08 => Key::Backspace,
            0x01 => Key::Home,
            0x05 => Key::End,
            0x15 => Key::KillLeft,
            0x0b => Key::KillRight,
            0x17 => Key::KillWord,
            0x04 => Key::Eof,
            ascii if ascii < 0x80 => Key::Char(ascii as char),
            lead => {
                // The first byte of a multi-byte character says how many follow.
                let more = match lead {
                    0xF0..=0xFF => 3,
                    0xE0..=0xEF => 2,
                    0xC0..=0xDF => 1,
                    _ => 0,
                };
                let mut bytes = vec![lead];
                for _ in 0..more {
                    match self.byte(false) {
                        Some(byte) => bytes.push(byte),
                        None => break,
                    }
                }
                match std::str::from_utf8(&bytes).ok().and_then(|text| text.chars().next()) {
                    Some(character) => Key::Char(character),
                    None => Key::Other,
                }
            }
        }
    }

    /// What follows an escape byte. Alone it is the Escape key; with more it is an arrow or
    /// another special key.
    fn escape(&mut self) -> Key {
        let Some(first) = self.byte(false) else { return Key::Esc };
        if first == b'\n' {
            return Key::Esc; // piped input: a line that holds only an escape
        }
        if first != b'[' && first != b'O' {
            self.unread = Some(first); // Escape, then an ordinary key
            return Key::Esc;
        }
        let mut sequence = vec![first];
        if let Some(mut byte) = self.byte(false) {
            sequence.push(byte);
            while byte.is_ascii_digit() || byte == b';' {
                match self.byte(false) {
                    Some(next) => {
                        sequence.push(next);
                        byte = next;
                    }
                    None => break,
                }
            }
        }
        match sequence.as_slice() {
            b"[A" | b"OA" => Key::Up,
            b"[B" | b"OB" => Key::Down,
            b"[C" | b"OC" => Key::Right,
            b"[D" | b"OD" => Key::Left,
            b"[Z" => Key::BackTab,
            b"[H" | b"OH" | b"[1~" | b"[7~" => Key::Home,
            b"[F" | b"OF" | b"[4~" | b"[8~" => Key::End,
            b"[3~" => Key::Delete,
            _ => Key::Other,
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        self.restore();
    }
}
