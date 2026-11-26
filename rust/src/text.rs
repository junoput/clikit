//! Small text helpers. Widths are counted in characters, not bytes.

pub(crate) fn len(text: &str) -> usize {
    text.chars().count()
}

pub(crate) fn pad(text: &str, width: usize) -> String {
    let mut padded = String::from(text);
    for _ in len(text)..width {
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
