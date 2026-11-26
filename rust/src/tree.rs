//! Trees. [`tree`] draws a list of paths, or the files under a directory ([`tree_dir`]), as a
//! tree on standard output. [`browse`] lets a person walk through one with the arrow keys and
//! returns the path they chose.
//!
//! A list of paths can describe anything that nests: the files in a backup, the keys of a
//! configuration, the chapters of a book. A row is `[path, note]`; the parts of a path are
//! separated by `/`, and a path that ends in `/` is a directory even when nothing is in it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ctx::{self, Mode, Theme, BLUE, BOLD, CYAN, DIM, GREEN, RESET};
use crate::keys::{Key, Keys};
use crate::msg::{msg, Level};
use crate::prompt::{keys_available, read_answer, PromptError};
use crate::text::{len, truncate};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
    /// A symbolic link to a directory: it can be opened, but a drawing does not follow it.
    Link,
}

struct Node {
    /// The name as text.
    name: String,
    /// The name as the file system has it. The top of a directory tree holds the path it was
    /// given as.
    real: OsString,
    parent: Option<usize>,
    children: Vec<usize>,
    kind: Kind,
    open: bool,
    /// The entries of this directory have been read.
    loaded: bool,
    note: String,
}

/// One line of a tree as it is shown.
struct Row {
    id: usize,
    /// The lines drawn before the name.
    draw: String,
    level: usize,
    /// There is more inside that is not shown.
    more: bool,
}

struct Forest {
    nodes: Vec<Node>,
    top: Vec<usize>,
    /// The nodes are files: a directory is read when somebody looks into it.
    files: bool,
    /// Leave out names that start with a dot.
    skip_dot: bool,
    only_dirs: bool,
}

impl Forest {
    fn add(&mut self, name: String, real: OsString, parent: Option<usize>, kind: Kind, loaded: bool) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            name,
            real,
            parent,
            children: Vec::new(),
            kind,
            open: false,
            loaded,
            note: String::new(),
        });
        match parent {
            Some(parent) => self.nodes[parent].children.push(id),
            None => self.top.push(id),
        }
        id
    }

    /// The tree that a list of paths describes. Children keep the order they first appear in.
    fn from_rows<R: AsRef<[C]>, C: AsRef<str>>(rows: &[R]) -> Forest {
        let mut forest = Forest { nodes: Vec::new(), top: Vec::new(), files: false, skip_dot: false, only_dirs: false };
        let mut ids: HashMap<(Option<usize>, String), usize> = HashMap::new();
        for row in rows {
            let cells = row.as_ref();
            let line = cells.first().map_or("", |cell| cell.as_ref().trim_matches(' '));
            let note = cells.get(1).map_or("", |cell| cell.as_ref().trim_matches(' '));
            let mut lead = if line.starts_with('/') { "/" } else { "" };
            let mut parent = None;
            for part in line.split('/').filter(|part| !part.is_empty() && *part != ".") {
                let name = format!("{lead}{part}");
                lead = "";
                let id = match ids.get(&(parent, name.clone())) {
                    Some(&id) => id,
                    None => {
                        let id = forest.add(name.clone(), OsString::from(&name), parent, Kind::File, true);
                        ids.insert((parent, name), id);
                        id
                    }
                };
                if let Some(parent) = parent {
                    forest.nodes[parent].kind = Kind::Dir;
                }
                parent = Some(id);
            }
            let Some(id) = parent else { continue };
            if line.ends_with('/') {
                forest.nodes[id].kind = Kind::Dir;
            }
            if forest.nodes[id].note.is_empty() {
                forest.nodes[id].note = note.to_string();
            }
        }
        forest
    }

    /// A tree with one node, the directory. The rest is read when it is looked at.
    fn from_dir(dir: OsString, hidden: bool) -> Forest {
        let mut forest =
            Forest { nodes: Vec::new(), top: Vec::new(), files: true, skip_dot: !hidden, only_dirs: false };
        let top = forest.add(dir.to_string_lossy().into_owned(), dir, None, Kind::Dir, false);
        forest.nodes[top].open = true;
        forest
    }

    /// The node's path, from the top of the tree.
    fn path(&self, id: usize) -> OsString {
        let mut path = self.nodes[id].real.clone();
        let mut at = self.nodes[id].parent;
        while let Some(id) = at {
            let mut longer = self.nodes[id].real.clone();
            if !self.nodes[id].name.ends_with('/') {
                longer.push("/");
            }
            longer.push(&path);
            path = longer;
            at = self.nodes[id].parent;
        }
        path
    }

    /// Read a directory's entries, once: directories first, each group in byte order.
    fn load(&mut self, id: usize) {
        if !self.files || self.nodes[id].loaded {
            return;
        }
        self.nodes[id].loaded = true;
        let dir = PathBuf::from(self.path(id));
        let Ok(entries) = fs::read_dir(&dir) else { return };
        let mut found: Vec<(bool, OsString, Kind)> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let full = dir.join(&name);
            let is_dir = fs::metadata(&full).map(|data| data.is_dir()).unwrap_or(false);
            let is_link = fs::symlink_metadata(&full).map(|data| data.file_type().is_symlink()).unwrap_or(false);
            let kind = match (is_dir, is_link) {
                (false, _) => Kind::File,
                (true, false) => Kind::Dir,
                (true, true) => Kind::Link,
            };
            found.push((!is_dir, name, kind));
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (_, name, kind) in found {
            self.add(name.to_string_lossy().into_owned(), name, Some(id), kind, false);
        }
    }

    fn shown(&self, id: usize) -> bool {
        let node = &self.nodes[id];
        if self.only_dirs && node.kind == Kind::File {
            return false;
        }
        !(self.skip_dot && node.parent.is_some() && node.name.starts_with('.'))
    }

    /// The name as shown: a directory ends in `/`, and a file name's control characters,
    /// which could be escape codes, are replaced.
    fn shown_name(&self, id: usize) -> String {
        let node = &self.nodes[id];
        let mut name = node.name.clone();
        if self.files {
            name = name.chars().map(|c| if c < ' ' || c == '\x7f' { '?' } else { c }).collect();
        }
        if node.kind != Kind::File && !name.ends_with('/') {
            name.push('/');
        }
        name
    }

    /// The rows on show. With `everything`, each directory down to `max` levels (0: all of
    /// them), but not through links; without, the open ones.
    fn walk(&mut self, max: usize, everything: bool) -> Vec<Row> {
        let mut rows = Vec::new();
        let top = self.top.clone();
        self.walk_into(&top, 1, "", max, everything, &mut rows);
        rows
    }

    fn walk_into(
        &mut self,
        among: &[usize],
        level: usize,
        before: &str,
        max: usize,
        everything: bool,
        rows: &mut Vec<Row>,
    ) {
        // A whole tree is content, on standard output; the open rows of a browser are drawn
        // on standard error.
        let t = if everything { ctx::theme_out() } else { ctx::theme() };
        let (mid, end, bar) = (t.g("├── ", "|-- "), t.g("╰── ", "`-- "), t.g("│   ", "|   "));
        let shown: Vec<usize> = among.iter().copied().filter(|&id| self.shown(id)).collect();
        for (at, &id) in shown.iter().enumerate() {
            let (draw, inside) = match (level, at + 1 == shown.len()) {
                (1, _) => (String::new(), String::new()),
                (_, true) => (format!("{before}{end}"), format!("{before}    ")),
                (_, false) => (format!("{before}{mid}"), format!("{before}{bar}")),
            };
            let kind = self.nodes[id].kind;
            let (mut children, mut more) = (Vec::new(), false);
            if kind != Kind::File && (everything || self.nodes[id].open) {
                self.load(id);
                children = self.nodes[id].children.clone();
                let any = children.iter().any(|&child| self.shown(child));
                if !any {
                    children.clear();
                } else if everything && (kind == Kind::Link || (max > 0 && level >= max)) {
                    more = true;
                    children.clear();
                }
            }
            rows.push(Row { id, draw, level, more });
            if !children.is_empty() {
                self.walk_into(&children, level + 1, &inside, max, everything, rows);
            }
        }
    }

    /// Put the directory above the top of the tree on top of it.
    fn up(&mut self) {
        let old = self.top[0];
        let Some(name) = self.nodes[old].real.to_str().map(String::from) else { return };
        let relative = name == "." || name == ".." || name.ends_with("/..");
        let above = match name.rfind('/') {
            _ if name == "/" => return,
            _ if name == "." => "..".to_string(),
            _ if relative => format!("{name}/.."),
            Some(0) => "/".to_string(),
            Some(slash) => name[..slash].to_string(),
            None => ".".to_string(),
        };
        let child = if relative {
            fs::canonicalize(&name).ok().and_then(|full| full.file_name().map(OsString::from))
        } else {
            Some(OsString::from(&name[name.rfind('/').map_or(0, |slash| slash + 1)..]))
        };
        let Some(child) = child.filter(|child| !child.is_empty()) else { return };
        if !Path::new(&above).is_dir() {
            return;
        }
        let new = self.add(above.clone(), OsString::from(above), None, Kind::Dir, false);
        self.nodes[new].open = true;
        self.top = vec![new];
        self.load(new);
        // The old top takes the place of its own entry.
        let entry = self.nodes[new].children.iter().position(|&id| self.nodes[id].real == child);
        if let Some(entry) = entry {
            self.nodes[new].children[entry] = old;
            self.nodes[old].parent = Some(new);
            self.nodes[old].name = child.to_string_lossy().into_owned();
            self.nodes[old].real = child;
        }
    }
}

/// A directory as it was given, without the slashes at its end.
fn without_slashes(dir: &Path) -> OsString {
    match dir.to_str() {
        Some(text) => {
            let mut text = text;
            while text.len() > 1 && text.ends_with('/') {
                text = &text[..text.len() - 1];
            }
            OsString::from(text)
        }
        None => dir.as_os_str().to_os_string(),
    }
}

/// `./name` without its `./`.
fn without_dot(path: &str) -> &str {
    match path.strip_prefix("./") {
        Some(rest) if !rest.is_empty() => rest,
        _ => path,
    }
}

fn dotless(path: OsString) -> PathBuf {
    match path.to_str() {
        Some(text) => PathBuf::from(without_dot(text)),
        None => PathBuf::from(path),
    }
}

/// A tree of paths, or of the files under a directory.
///
/// ```
/// use clikit::Tree;
///
/// let files = [["src/main.rs", "1.2 KB"], ["src/lib/mod.rs", "300 B"], ["README.md", "2 KB"]];
/// Tree::new().print(&files);            // the whole tree
/// Tree::new().depth(1).print(&files);   // one level: `src/ …` and `README.md`
/// ```
#[derive(Clone, Debug, Default)]
pub struct Tree {
    depth: usize,
    hidden: bool,
}

impl Tree {
    pub fn new() -> Tree {
        Tree::default()
    }

    /// Show this many levels (0, the default: all of them). A directory with more inside ends
    /// in `…`. For [`Tree::print_dir`] the levels are counted under the directory.
    pub fn depth(mut self, levels: usize) -> Tree {
        self.depth = levels;
        self
    }

    /// With [`Tree::print_dir`]: show the files whose names start with a dot too.
    pub fn hidden(mut self, show: bool) -> Tree {
        self.hidden = show;
        self
    }

    /// Print the tree that the rows describe. A row is `[path, note]`; the note is optional.
    pub fn print<R: AsRef<[C]>, C: AsRef<str>>(&self, rows: &[R]) {
        draw(Forest::from_rows(rows), self.depth);
    }

    /// Print the files under `dir`: directories first, then files, each in byte order.
    /// Symbolic links to directories are shown and not followed.
    ///
    /// When `dir` is not a directory this prints an error and returns one.
    pub fn print_dir(&self, dir: impl AsRef<Path>) -> io::Result<()> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            let problem = format!("tree: not a directory: {}", dir.display());
            msg(Level::Error, &problem);
            return Err(io::Error::new(io::ErrorKind::InvalidInput, problem));
        }
        let depth = if self.depth > 0 { self.depth + 1 } else { 0 };
        draw(Forest::from_dir(without_slashes(dir), self.hidden), depth);
        Ok(())
    }
}

/// Print a tree of paths. A row is `[path, note]`; the note is optional.
///
/// ```
/// clikit::tree(&[["src/main.rs", "1.2 KB"], ["src/lib.rs", "300 B"], ["README.md", ""]]);
/// ```
pub fn tree<R: AsRef<[C]>, C: AsRef<str>>(rows: &[R]) {
    Tree::new().print(rows);
}

/// Print the files under a directory as a tree. See [`Tree::print_dir`].
pub fn tree_dir(dir: impl AsRef<Path>) -> io::Result<()> {
    Tree::new().print_dir(dir)
}

fn draw(mut forest: Forest, max: usize) {
    let t = ctx::theme_out();
    let rows = forest.walk(max, true);
    let ellipsis = t.g("…", "...");
    let (dim, blue, reset) = (t.c(DIM), t.c(BLUE), t.c(RESET));
    let names: Vec<String> = rows.iter().map(|row| forest.shown_name(row.id)).collect();
    let width = |row: &Row, name: &str| len(&row.draw) + len(name) + if row.more { 1 + len(ellipsis) } else { 0 };
    // The notes start in one column, after the longest name that has one.
    let notes_at = rows
        .iter()
        .zip(&names)
        .filter(|(row, _)| !forest.nodes[row.id].note.is_empty())
        .map(|(row, name)| width(row, name))
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for (row, name) in rows.iter().zip(&names) {
        let node = &forest.nodes[row.id];
        if t.mode == Mode::Agent {
            text.push_str(&" ".repeat(row.level - 1));
            text.push_str(name);
            if row.more {
                text.push_str(&format!(" {ellipsis}"));
            }
            if !node.note.is_empty() {
                text.push_str(&format!("\t{}", node.note));
            }
        } else {
            if !row.draw.is_empty() {
                text.push_str(&format!("{dim}{}{reset}", row.draw));
            }
            if node.kind == Kind::File {
                text.push_str(name);
            } else {
                text.push_str(&format!("{blue}{name}{reset}"));
            }
            if row.more {
                text.push_str(&format!(" {dim}{ellipsis}{reset}"));
            }
            if !node.note.is_empty() {
                text.push_str(&" ".repeat(notes_at - width(row, name)));
                text.push_str(&format!("  {dim}{}{reset}", node.note));
            }
        }
        text.push('\n');
    }
    ctx::out(&text);
}

/// Walk through a tree with the keys and choose a path.
///
/// | Key | Does |
/// |---|---|
/// | up, down (`k`, `j`, Shift-Tab, Tab) | move |
/// | right (`l`) | open a directory; on an open one, step inside |
/// | left (`h`) | close a directory; otherwise go to the directory around; on the top one, show the directory above it |
/// | Enter | choose; on a directory, when files are being chosen, open or close it |
/// | Space | open or close a directory |
/// | `.` | show or hide the files whose names start with a dot |
/// | Home, End | the first row, the last row |
/// | Escape | cancel ([`PromptError::Cancelled`]) |
///
/// Without a terminal it reads one line, a path, and checks that it exists.
///
/// ```no_run
/// use clikit::Browse;
///
/// let target = Browse::new().directories(true).ask("Restore to", ".")?;
/// let file = Browse::new().ask_rows("Restore which file", &[["Photos/a.jpg", "2 MB"], ["Notes/todo.txt", "4 KB"]])?;
/// # let _ = (target, file);
/// # Ok::<(), clikit::PromptError>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct Browse {
    directories: bool,
    hidden: bool,
}

impl Browse {
    pub fn new() -> Browse {
        Browse::default()
    }

    /// Choose a directory, not a file. Files are not shown.
    pub fn directories(mut self, only: bool) -> Browse {
        self.directories = only;
        self
    }

    /// Show the files whose names start with a dot from the start.
    pub fn hidden(mut self, show: bool) -> Browse {
        self.hidden = show;
        self
    }

    /// Walk through the files under `dir`. The answer starts with `dir` as it was given
    /// (without a leading `./`).
    pub fn ask(&self, question: &str, dir: impl AsRef<Path>) -> Result<PathBuf, PromptError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            msg(Level::Error, &format!("{question}: not a directory: {}", dir.display()));
            return Err(PromptError::NoOptions);
        }
        let dir = without_slashes(dir);
        let shown = dir.to_string_lossy().into_owned();
        let mut forest = Forest::from_dir(dir, self.hidden);
        forest.only_dirs = self.directories;
        ask(question, forest, &shown)
    }

    /// Walk through a list of paths. A row is `[path, note]`; the note is optional.
    pub fn ask_rows<R: AsRef<[C]>, C: AsRef<str>>(&self, question: &str, rows: &[R]) -> Result<String, PromptError> {
        let mut forest = Forest::from_rows(rows);
        forest.only_dirs = self.directories;
        ask(question, forest, "").map(|chosen| chosen.to_string_lossy().into_owned())
    }
}

/// Walk through the files under `dir` and choose one. See [`Browse`] for the keys.
///
/// ```no_run
/// let file = clikit::browse("Open", ".")?;
/// # let _ = file;
/// # Ok::<(), clikit::PromptError>(())
/// ```
pub fn browse(question: &str, dir: impl AsRef<Path>) -> Result<PathBuf, PromptError> {
    Browse::new().ask(question, dir)
}

fn ask(question: &str, mut forest: Forest, dir: &str) -> Result<PathBuf, PromptError> {
    let rows = forest.walk(0, false);
    if rows.is_empty() {
        msg(Level::Error, &format!("{question}: nothing to choose from"));
        return Err(PromptError::NoOptions);
    }
    if keys_available() {
        ask_keys(question, &mut forest, rows, ctx::theme())
    } else {
        ask_line(question, &forest, dir)
    }
}

fn ask_keys(title: &str, forest: &mut Forest, mut rows: Vec<Row>, t: Theme) -> Result<PathBuf, PromptError> {
    let page = ctx::env("CLI_PAGE").and_then(|page| page.parse::<usize>().ok()).filter(|page| *page > 0).unwrap_or(10);
    let max_width = ctx::width() as isize - 3;
    let (cyan, bold, dim, green, blue, reset) = (t.c(CYAN), t.c(BOLD), t.c(DIM), t.c(GREEN), t.c(BLUE), t.c(RESET));
    let mut hint = String::from(
        t.g("↑/↓ move, → open, ← close, enter select", "up/down move, right open, left close, enter select"),
    );
    if forest.files {
        hint.push_str(", . hidden files");
    }
    ctx::err(&format!("{cyan}?{reset} {bold}{title}{reset}  {dim}{hint}{reset}\n"));

    let mut keys = Keys::open();
    let (mut cur, mut top) = (0, 0);
    // After a change to the tree: the node the cursor should be on.
    let mut find: Option<usize> = None;
    let (chosen, drawn) = loop {
        if let Some(mut wanted) = find.take() {
            rows = forest.walk(0, false);
            cur = loop {
                if let Some(at) = rows.iter().position(|row| row.id == wanted) {
                    break at;
                }
                // Its row is gone: the directory it was in.
                match forest.nodes[wanted].parent {
                    Some(parent) => wanted = parent,
                    None => break 0,
                }
            };
        }
        let n = rows.len();
        if cur < top {
            top = cur;
        }
        if cur >= top + page {
            top = cur + 1 - page;
        }
        if top + page > n {
            top = n.saturating_sub(page);
        }
        let last = (top + page).min(n);
        let mut frame = String::new();
        for (at, row) in rows.iter().enumerate().take(last).skip(top) {
            let node = &forest.nodes[row.id];
            let name = forest.shown_name(row.id);
            let room = (max_width - len(&row.draw) as isize).max(1) as usize;
            let name = truncate(&name, room);
            let pointer = if at == cur { format!("{cyan}{}{reset} ", t.g("❯", ">")) } else { "  ".to_string() };
            let lines = if row.draw.is_empty() { String::new() } else { format!("{dim}{}{reset}", row.draw) };
            let mut style = String::new();
            if at == cur {
                style.push_str(bold);
            }
            if node.kind != Kind::File {
                style.push_str(blue);
            }
            let unstyle = if style.is_empty() { "" } else { reset };
            let note = if !node.note.is_empty() && len(name) + 2 + len(&node.note) <= room {
                format!("  {dim}{}{reset}", node.note)
            } else {
                String::new()
            };
            frame.push_str(&format!("{pointer}{lines}{style}{name}{unstyle}{note}\x1b[K\n"));
        }
        let mut drawn = last - top;
        if n > page {
            frame.push_str(&format!("  {dim}{}-{last} of {n}{reset}\x1b[K\n", top + 1));
            drawn += 1;
        }
        frame.push_str("\x1b[J");
        ctx::err(&frame);

        let key = keys.next();
        let id = rows[cur].id;
        let (kind, open, parent) = (forest.nodes[id].kind, forest.nodes[id].open, forest.nodes[id].parent);
        // The directory that was asked about is always open.
        let is_top = forest.files && parent.is_none();
        let folds = kind != Kind::File && !is_top;
        match key {
            Key::Up | Key::Char('k') | Key::BackTab => cur = if cur == 0 { n - 1 } else { cur - 1 },
            Key::Down | Key::Char('j') | Key::Tab => cur = if cur + 1 == n { 0 } else { cur + 1 },
            Key::Home => cur = 0,
            Key::End => cur = n - 1,
            Key::Right | Key::Char('l') if kind != Kind::File => {
                if !open {
                    forest.nodes[id].open = true;
                    find = Some(id);
                } else if cur + 1 < n && forest.nodes[rows[cur + 1].id].parent == Some(id) {
                    cur += 1; // open already: step inside
                }
            }
            Key::Left | Key::Char('h') => {
                if is_top {
                    forest.up();
                    find = Some(id);
                } else if kind != Kind::File && open {
                    forest.nodes[id].open = false;
                    find = Some(id);
                } else if let Some(parent) = parent {
                    find = Some(parent);
                }
            }
            Key::Space if folds => {
                forest.nodes[id].open = !open;
                find = Some(id);
            }
            Key::Char('.') if forest.files => {
                forest.skip_dot = !forest.skip_dot;
                find = Some(id);
            }
            Key::Enter => {
                if kind == Kind::File || forest.only_dirs {
                    break (id, drawn);
                }
                if folds {
                    forest.nodes[id].open = !open;
                    find = Some(id);
                }
            }
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

    let path = if forest.files { dotless(forest.path(chosen)) } else { PathBuf::from(forest.path(chosen)) };
    ctx::err(&format!(
        "\x1b[{}A\x1b[J{green}{}{reset} {title} {dim}{}{reset} {}\n",
        drawn + 1,
        t.g("✓", "+"),
        t.g("›", ":"),
        path.display(),
    ));
    Ok(path)
}

/// The same question without a terminal: one line in, a path.
fn ask_line(question: &str, forest: &Forest, dir: &str) -> Result<PathBuf, PromptError> {
    let agent = ctx::theme().mode == Mode::Agent;
    let mut what = String::from(if forest.only_dirs { "a directory" } else { "a file" });
    if forest.files {
        what.push_str(&format!(" under {dir}"));
    }
    if !agent {
        ctx::err(&format!("{question} ({what}): "));
    }
    let answer = read_answer().unwrap_or_default();
    if !agent && !ctx::stdin_tty() {
        ctx::err(&format!("{answer}\n"));
    }
    let mut hit: Option<String> = None;
    if forest.files {
        // As typed, or under the directory.
        for candidate in [answer.clone(), format!("{dir}/{answer}")] {
            let found = Path::new(&candidate);
            let fits = if forest.only_dirs { found.is_dir() } else { found.exists() && !found.is_dir() };
            if !answer.is_empty() && fits {
                hit = Some(without_dot(&candidate).to_string());
                break;
            }
        }
    } else {
        let mut options = Vec::new();
        for (id, node) in forest.nodes.iter().enumerate() {
            if (node.kind == Kind::File) == forest.only_dirs {
                continue;
            }
            let path = forest.path(id).to_string_lossy().into_owned();
            if path == answer || format!("{path}/") == answer {
                hit = Some(path.clone());
            }
            options.push(path);
        }
        what = format!("{what}: {}", options.join(" | "));
    }
    if answer.is_empty() {
        msg(Level::Error, &format!("input required: {question} ({what}). Pipe the path on stdin."));
        return Err(PromptError::NoAnswer);
    }
    let Some(hit) = hit else {
        msg(Level::Error, &format!("invalid choice '{answer}' for: {question} ({what})"));
        return Err(PromptError::Invalid(answer));
    };
    if agent {
        ctx::err(&format!("{question}: {hit}\n"));
    }
    Ok(PathBuf::from(hit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_become_a_tree() {
        let forest = Forest::from_rows(&[["b/x", "1"], ["a/", ""], ["b/y", ""], ["/abs/./z", ""], ["b/x", "2"]]);
        let names: Vec<&str> = forest.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["b", "x", "a", "y", "/abs", "z"]);
        assert_eq!(forest.top, [0, 2, 4]);
        assert_eq!(forest.nodes[0].children, [1, 3]);
        assert!(forest.nodes[2].kind == Kind::Dir && forest.nodes[1].kind == Kind::File);
        assert_eq!(forest.nodes[1].note, "1");
        assert_eq!(forest.path(5), OsString::from("/abs/z"));
    }

    #[test]
    fn a_given_directory_loses_its_last_slashes_only() {
        assert_eq!(without_slashes(Path::new("a/b//")), OsString::from("a/b"));
        assert_eq!(without_slashes(Path::new("/")), OsString::from("/"));
        assert_eq!(without_dot("./a"), "a");
        assert_eq!(without_dot("./"), "./");
        assert_eq!(without_dot("a/./b"), "a/./b");
    }
}
