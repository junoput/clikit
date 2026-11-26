//! Small text helpers. Widths are counted in characters, not bytes.

pub(crate) fn len(text: &str) -> usize {
    text.chars().count()
}

/// The columns `text` takes on a terminal. Escape sequences take none, marks that sit on the
/// character before them take none, CJK and most emoji take two.
pub(crate) fn width(text: &str) -> usize {
    visible(text).map(columns).sum()
}

/// The characters of `text` without its escape sequences (`ESC [ ... letter`).
fn visible(text: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = text.chars().peekable();
    std::iter::from_fn(move || loop {
        let c = chars.next()?;
        if c != '\x1b' {
            return Some(c);
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for inside in chars.by_ref() {
                if inside.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    })
}

fn columns(c: char) -> usize {
    let c = c as u32;
    match c {
        0..=0x1F | 0x7F => 0,
        0x300..=0x36F | 0x200B..=0x200F | 0x20D0..=0x20FF | 0xFE00..=0xFE0F => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// As much of `text` as fits in `room` columns. Escape sequences are dropped: a cut in the
/// middle of coloured text would leave the colour on.
pub(crate) fn cut(text: &str, room: usize) -> String {
    let (mut kept, mut used) = (String::new(), 0);
    for c in visible(text) {
        if used + columns(c) > room {
            break;
        }
        used += columns(c);
        kept.push(c);
    }
    kept
}

pub(crate) fn pad(text: &str, columns: usize) -> String {
    let mut padded = String::from(text);
    for _ in width(text)..columns {
        padded.push(' ');
    }
    padded
}

pub(crate) fn truncate(text: &str, width: usize) -> &str {
    match text.char_indices().nth(width) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// A number of bytes as text: `512 B`, `4.2 KB`, `28.0 GB`. Steps of 1024, one decimal, rounded.
///
/// ```
/// assert_eq!(clikit::size(1536), "1.5 KB");
/// assert_eq!(clikit::size(30_000_000_000), "27.9 GB");
/// ```
pub fn size(bytes: u64) -> String {
    const NAMES: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    let n = u128::from(bytes);
    let (mut unit, mut name, mut tenths) = (1u128, "B", 0);
    for i in 0..=NAMES.len() {
        let next = NAMES.get(i).copied();
        if next.is_none() || n < unit * 1024 {
            if unit == 1 {
                return format!("{n} B");
            }
            tenths = (n * 10 + unit / 2) / unit;
            // 1024.0 is 1.0 of the next unit.
            if tenths < 10240 || next.is_none() {
                break;
            }
        }
        (unit, name) = (unit * 1024, next.unwrap_or(name));
    }
    format!("{}.{} {name}", tenths / 10, tenths % 10)
}

/// Split a line into words the way a shell would: spaces separate, quotes group,
/// a backslash takes the next character literally.
///
/// ```
/// let words = clikit::split_words(r#"add "Buy milk" -p 1"#).unwrap();
/// assert_eq!(words, ["add", "Buy milk", "-p", "1"]);
/// ```
pub fn split_words(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err("unterminated single quote".into()),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err("unterminated double quote".into()),
                        },
                        Some(inner) => word.push(inner),
                        None => return Err("unterminated double quote".into()),
                    }
                }
            }
            '\\' => {
                started = true;
                match chars.next() {
                    Some(escaped) => word.push(escaped),
                    None => return Err("trailing backslash".into()),
                }
            }
            other => {
                started = true;
                word.push(other);
            }
        }
    }
    if started {
        words.push(word);
    }
    Ok(words)
}
