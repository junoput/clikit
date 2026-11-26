//! Content: headers, boxes, lists, key/values and tables. It goes to standard output.

use std::fmt::Display;

use crate::ctx::{self, Mode, BOLD, CYAN, DIM, GRAY, RESET};
use crate::text::{len, pad, width};

/// A blank line, for people only.
pub fn nl() {
    if ctx::theme_out().mode != Mode::Agent {
        ctx::out("\n");
    }
}

/// A section title.
pub fn header(title: impl Display) {
    let t = ctx::theme_out();
    let title = title.to_string();
    let text = match t.mode {
        Mode::Agent => format!("# {title}\n"),
        Mode::Plain => format!("\n== {title} ==\n"),
        Mode::Rich => {
            let rule = t.g("─", "-").repeat(len(&title));
            format!("\n{}{title}{}\n{}{rule}{}\n", t.c(BOLD), t.c(RESET), t.c(DIM), t.c(RESET))
        }
    };
    ctx::out(&text);
}

/// A horizontal rule, for people only.
pub fn hr() {
    let t = ctx::theme_out();
    if t.mode == Mode::Agent {
        return;
    }
    let rule = t.g("─", "-").repeat(ctx::width().min(80));
    ctx::out(&format!("{}{rule}{}\n", t.c(DIM), t.c(RESET)));
}

/// Plain text in a frame, with an optional title. Agents get the text without the frame.
pub fn boxed<S: AsRef<str>>(title: Option<&str>, lines: &[S]) {
    let t = ctx::theme_out();
    let title = title.unwrap_or("");
    let mut text = String::new();
    if t.mode == Mode::Agent {
        if !title.is_empty() {
            text.push_str(&format!("{title}:\n"));
        }
        for line in lines {
            text.push_str(line.as_ref());
            text.push('\n');
        }
        ctx::out(&text);
        return;
    }
    let mut w = lines.iter().map(|line| width(line.as_ref())).max().unwrap_or(0);
    if !title.is_empty() {
        w = w.max(width(title) + 2);
    }
    let inner = w + 2;
    let (h, v) = (t.g("─", "-"), t.g("│", "|"));
    let (gray, reset) = (t.c(GRAY), t.c(RESET));
    let top = if title.is_empty() {
        h.repeat(inner)
    } else {
        format!("{h} {reset}{}{title}{reset}{gray} {}", t.c(BOLD), h.repeat(inner - width(title) - 3))
    };
    text.push_str(&format!("{gray}{}{top}{}{reset}\n", t.g("╭", "+"), t.g("╮", "+")));
    for line in lines {
        text.push_str(&format!("{gray}{v}{reset} {} {gray}{v}{reset}\n", pad(line.as_ref(), w)));
    }
    text.push_str(&format!("{gray}{}{}{}{reset}\n", t.g("╰", "+"), h.repeat(inner), t.g("╯", "+")));
    ctx::out(&text);
}

fn print_list<S: AsRef<str>>(items: &[S], numbered: bool) {
    let t = ctx::theme_out();
    let digits = items.len().to_string().len();
    let (cyan, reset) = (t.c(CYAN), t.c(RESET));
    let mut text = String::new();
    for (i, item) in items.iter().enumerate() {
        let (i, item) = (i + 1, item.as_ref());
        let line = match (t.mode, numbered) {
            (Mode::Agent, true) => format!("{i}. {item}\n"),
            (Mode::Agent, false) => format!("- {item}\n"),
            (_, true) => format!("  {cyan}{i:>digits$}.{reset} {item}\n"),
            (_, false) => format!("  {cyan}{}{reset} {item}\n", t.g("•", "-")),
        };
        text.push_str(&line);
    }
    ctx::out(&text);
}

/// A bulleted list.
pub fn list<S: AsRef<str>>(items: &[S]) {
    print_list(items, false);
}

/// A numbered list.
pub fn list_numbered<S: AsRef<str>>(items: &[S]) {
    print_list(items, true);
}

/// Aligned `key: value` lines.
pub fn kv<K: AsRef<str>, V: AsRef<str>>(pairs: &[(K, V)]) {
    let t = ctx::theme_out();
    let w = pairs.iter().map(|(key, _)| width(key.as_ref())).max().unwrap_or(0);
    let mut text = String::new();
    for (key, value) in pairs {
        let (key, value) = (key.as_ref(), value.as_ref());
        if t.mode == Mode::Agent {
            text.push_str(&format!("{key}: {value}\n"));
        } else {
            let key = pad(&format!("{key}:"), w + 1);
            text.push_str(&format!("  {}{key}{} {value}\n", t.c(DIM), t.c(RESET)));
        }
    }
    ctx::out(&text);
}

/// A table with options. [`table`] is the short form for the defaults.
///
/// ```
/// use clikit::Table;
///
/// Table::new().border(true).align("lr").print(&[["Item", "Qty"], ["Apple", "3"], ["Melon", "12"]]);
/// ```
#[derive(Clone, Debug)]
pub struct Table {
    align: String,
    border: bool,
    header: bool,
    keep: Vec<String>,
}

impl Default for Table {
    fn default() -> Table {
        Table::new()
    }
}

impl Table {
    /// The first row is a header, no borders, numbers aligned right and everything else left.
    pub fn new() -> Table {
        Table { align: String::new(), border: false, header: true, keep: Vec::new() }
    }

    /// Alignment per column: `l`, `r` or `c`. `"lrc"` is left, right, centred. A column the
    /// text does not reach keeps the default: right when it holds only numbers, otherwise left.
    pub fn align(mut self, columns: &str) -> Table {
        self.align = columns.to_string();
        self
    }

    /// Never cut this column when the table is too wide for the terminal: an identifier, say,
    /// that is useless without its end. `column` is the column's header, or its number
    /// counted from 1. Call it once per column. When nothing is left to cut, the table runs
    /// past the edge.
    pub fn keep(mut self, column: &str) -> Table {
        self.keep.push(column.to_string());
        self
    }

    /// Draw borders around the cells.
    pub fn border(mut self, on: bool) -> Table {
        self.border = on;
        self
    }

    /// Whether the first row is a header. Without one, every row is data: aligned columns.
    pub fn header(mut self, on: bool) -> Table {
        self.header = on;
        self
    }

    /// Print the rows. Cells are plain text; they are trimmed and aligned. At a terminal a
    /// table wider than the screen has its widest columns cut. Agents get tab-separated rows.
    pub fn print<R: AsRef<[C]>, C: AsRef<str>>(&self, rows: &[R]) {
        let t = ctx::theme_out();
        if rows.is_empty() {
            return;
        }
        // A row without cells is a row with one empty cell.
        let rows: Vec<Vec<&str>> = rows
            .iter()
            .map(|row| {
                let cells: Vec<&str> = row.as_ref().iter().map(|cell| cell.as_ref().trim_matches(' ')).collect();
                if cells.is_empty() {
                    vec![""]
                } else {
                    cells
                }
            })
            .collect();
        let mut text = String::new();
        if t.mode == Mode::Agent {
            for row in &rows {
                text.push_str(&format!("{}\n", row.join("\t")));
            }
            ctx::out(&text);
            return;
        }

        // The width of each column, and whether it holds only numbers.
        let mut widths: Vec<usize> = Vec::new();
        let mut numeric: Vec<Option<bool>> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for (i, cell) in row.iter().enumerate() {
                if i == widths.len() {
                    widths.push(0);
                    numeric.push(None);
                }
                widths[i] = widths[i].max(width(cell));
                if !cell.is_empty() && numeric[i] != Some(false) && (r > 0 || !self.header) {
                    numeric[i] = Some(is_number(cell));
                }
            }
        }
        let n = widths.len();
        let aligns: Vec<char> = (0..n)
            .map(|i| match self.align.chars().nth(i) {
                Some(given @ ('l' | 'r' | 'c')) => given,
                _ if numeric[i] == Some(true) => 'r',
                _ => 'l',
            })
            .collect();
        // The columns that are never cut: by header first, then by number.
        let mut kept = vec![false; n];
        for column in &self.keep {
            let by_header = if self.header { rows[0].iter().position(|cell| cell == column) } else { None };
            let by_number = || column.parse::<usize>().ok().filter(|i| (1..=n).contains(i)).map(|i| i - 1);
            let digits = !column.is_empty() && column.bytes().all(|b| b.is_ascii_digit());
            if let Some(i) = by_header.or_else(|| if digits { by_number() } else { None }) {
                kept[i] = true;
            }
        }
        if t.mode == Mode::Rich {
            // Too wide for the terminal: shorten the widest column that may be cut.
            let frame = if self.border { 3 * n + 1 } else { 2 * n };
            let mut over = (widths.iter().sum::<usize>() + frame) as isize - ctx::width() as isize;
            while over > 0 {
                let mut widest: Option<usize> = None;
                for i in 0..n {
                    if !kept[i] && widths[i] > widest.map_or(0, |widest| widths[widest]) {
                        widest = Some(i);
                    }
                }
                // Nothing left to cut: the table runs past the edge.
                let Some(widest) = widest.filter(|widest| widths[*widest] > 4) else { break };
                widths[widest] -= 1;
                over -= 1;
            }
        }

        let h = t.g("─", "-");
        let (gray, bold, dim, reset) = (t.c(GRAY), t.c(BOLD), t.c(DIM), t.c(RESET));
        let bar = format!("{gray}{}{reset}", t.g("│", "|"));
        let rule = |left: &str, middle: &str, right: &str| {
            let cells: Vec<String> = widths.iter().map(|w| h.repeat(*w)).collect();
            format!("{gray}{left}{h}{}{h}{right}{reset}\n", cells.join(&format!("{h}{middle}{h}")))
        };
        if self.border {
            text.push_str(&rule(t.g("╭", "+"), t.g("┬", "+"), t.g("╮", "+")));
        }
        for (r, row) in rows.iter().enumerate() {
            let heading = r == 0 && self.header;
            let mut line = String::new();
            if self.border {
                for i in 0..n {
                    let cell = fit(row.get(i).copied().unwrap_or(""), widths[i], aligns[i], t.g("…", "~"));
                    if heading {
                        line.push_str(&format!("{bar} {bold}{cell}{reset} "));
                    } else {
                        line.push_str(&format!("{bar} {cell} "));
                    }
                }
                text.push_str(&format!("{line}{bar}\n"));
                if heading && rows.len() > 1 {
                    text.push_str(&rule(t.g("├", "+"), t.g("┼", "+"), t.g("┤", "+")));
                }
                continue;
            }
            for (i, cell) in row.iter().enumerate() {
                line.push_str(&fit(cell, widths[i], aligns[i], t.g("…", "~")));
                if i + 1 < row.len() {
                    line.push_str("  ");
                }
            }
            let line = line.trim_end_matches(' ');
            if heading {
                let rule: Vec<String> = widths.iter().map(|w| h.repeat(*w)).collect();
                text.push_str(&format!("  {bold}{line}{reset}\n  {dim}{}{reset}\n", rule.join("  ")));
            } else {
                text.push_str(&format!("  {line}\n"));
            }
        }
        if self.border {
            text.push_str(&rule(t.g("╰", "+"), t.g("┴", "+"), t.g("╯", "+")));
        }
        ctx::out(&text);
    }
}

/// Cut `text` to `width` if it is longer, then pad it: `l` left, `r` right, `c` centred.
pub(crate) fn fit(text: &str, columns: usize, align: char, ellipsis: &str) -> String {
    let text = if width(text) > columns {
        format!("{}{ellipsis}", crate::text::cut(text, columns.saturating_sub(1)))
    } else {
        text.to_string()
    };
    let gap = columns.saturating_sub(width(&text));
    match align {
        'r' => format!("{}{text}", " ".repeat(gap)),
        'c' => format!("{}{text}{}", " ".repeat(gap / 2), " ".repeat(gap - gap / 2)),
        _ => format!("{text}{}", " ".repeat(gap)),
    }
}

/// `12`, `-3`, `1,000.50`, `50%`: digits with `.` or `,` between groups, an optional sign and
/// an optional `%`.
fn is_number(text: &str) -> bool {
    let text = text.strip_prefix(['-', '+']).unwrap_or(text);
    let text = text.strip_suffix('%').unwrap_or(text);
    !text.is_empty()
        && text.split(['.', ',']).all(|group| !group.is_empty() && group.bytes().all(|b| b.is_ascii_digit()))
}

/// A table. The first row is the header. Columns that hold only numbers are aligned right.
/// See [`Table`] for borders, explicit alignment and tables without a header.
///
/// ```
/// clikit::table(&[["Name", "Role"], ["Ana", "dev"], ["Björn", "ops"]]);
/// ```
pub fn table<R: AsRef<[C]>, C: AsRef<str>>(rows: &[R]) {
    Table::new().print(rows);
}

/// Text with styles applied, or the text unchanged when colours are off.
///
/// Styles are separated by spaces: `bold dim italic underline inverse`, a colour
/// (`black red green yellow blue magenta cyan white gray`), or `bg-<colour>`.
pub fn style(styles: &str, text: impl Display) -> String {
    let t = ctx::theme_out();
    if !t.color {
        return text.to_string();
    }
    let mut seq = String::new();
    for name in styles.split(' ') {
        let code = match name {
            "bold" => 1,
            "dim" => 2,
            "italic" => 3,
            "underline" => 4,
            "inverse" => 7,
            "black" => 30,
            "red" => 31,
            "green" => 32,
            "yellow" => 33,
            "blue" => 34,
            "magenta" => 35,
            "cyan" => 36,
            "white" => 37,
            "gray" | "grey" => 90,
            "bg-black" => 40,
            "bg-red" => 41,
            "bg-green" => 42,
            "bg-yellow" => 43,
            "bg-blue" => 44,
            "bg-magenta" => 45,
            "bg-cyan" => 46,
            "bg-white" => 47,
            _ => continue,
        };
        seq.push_str(&format!("\x1b[{code}m"));
    }
    format!("{seq}{text}{RESET}")
}
