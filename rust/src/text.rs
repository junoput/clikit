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
