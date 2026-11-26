//! For applications that parse their arguments with [clap](https://docs.rs/clap).
//!
//! clap keeps doing the parsing and the help pages people read. This module adds what an
//! agent needs and what the shell implementation gives for free:
//!
//! - [`parse`]: `--agent`, `--plain` and `--no-color` anywhere on the command line; in agent
//!   mode, help is one compact page and errors are one line.
//! - [`agent_help`]: that compact page, for any `clap::Command`.
//! - [`interactive`]: a prompt that runs the application's commands until `exit`.
//!
//! ```no_run
//! use clap::Parser;
//!
//! #[derive(Parser)]
//! #[command(name = "todo", version, about = "A tiny todo list")]
//! enum Todo {
//!     /// Add a task
//!     Add { text: String },
//!     /// Show the tasks
//!     List,
//! }
//!
//! match clikit::clap::parse::<Todo>() {
//!     Todo::Add { text } => clikit::ok(format!("Added: {text}")),
//!     Todo::List => clikit::list(&["Buy milk"]),
//! }
//! ```

use ::clap::error::{ContextKind, ErrorKind};
use ::clap::{Arg, Command, Parser};

use crate::ctx::{self, Mode, CYAN, RESET};
use crate::edit::{Candidate, Completion, Editor, Read};
use crate::msg::{msg, Level};

/// Parse the process's arguments, or print help or an error and exit.
pub fn parse<T: Parser>() -> T {
    parse_from(std::env::args())
}

/// Like [`parse`], for arguments from somewhere else. The first one is the program name.
pub fn parse_from<T: Parser>(args: impl IntoIterator<Item = impl Into<String>>) -> T {
    match try_parse_from(args) {
        Ok(parsed) => parsed,
        Err(code) => std::process::exit(code),
    }
}

/// Like [`parse_from`], but returns the exit status instead of exiting. Help or the error has
/// been printed by then.
pub fn try_parse_from<T: Parser>(args: impl IntoIterator<Item = impl Into<String>>) -> Result<T, i32> {
    let mut args: Vec<String> = args.into_iter().map(Into::into).collect();
    take_mode_flags(&mut args);
    let command = T::command();
    let error = match command.clone().try_get_matches_from(&args).and_then(|m| T::from_arg_matches(&m)) {
        Ok(parsed) => return Ok(parsed),
        Err(error) => error,
    };
    if !ctx::is_agent() {
        let _ = error.print();
        return Err(error.exit_code());
    }
    match error.kind() {
        ErrorKind::DisplayHelp => {
            ctx::out(&agent_help_at(&command, &subcommand_path(&command, &args)));
            Err(0)
        }
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            ctx::out(&agent_help_at(&command, &subcommand_path(&command, &args)));
            Err(2)
        }
        ErrorKind::DisplayVersion => {
            ctx::out(&error.render().to_string());
            Err(0)
        }
        ErrorKind::InvalidSubcommand => {
            let wrong = error.get(ContextKind::InvalidSubcommand).map(|v| v.to_string()).unwrap_or_default();
            let names: Vec<&str> = visible_subcommands(&command).map(|c| c.get_name()).collect();
            msg(Level::Error, &format!("unknown command '{wrong}'; commands: {}", names.join(" ")));
            Err(2)
        }
        _ => {
            // clap's first line says what is wrong; the usage and tips that follow are for people.
            let text = error.render().to_string();
            let first = text.lines().next().unwrap_or("invalid arguments");
            msg(Level::Error, first.strip_prefix("error: ").unwrap_or(first));
            Err(2)
        }
    }
}

/// Remove `--agent`, `--plain` and `--no-color` (anywhere before `--`) and apply them.
fn take_mode_flags(args: &mut Vec<String>) {
    let end = args.iter().position(|arg| arg == "--").unwrap_or(args.len());
    let mut i = 1;
    let mut end = end;
    while i < end {
        match args[i].as_str() {
            "--agent" => ctx::set_mode(Mode::Agent),
            "--plain" => ctx::set_mode(Mode::Plain),
            "--no-color" => ctx::set_color(false),
            _ => {
                i += 1;
                continue;
            }
        }
        args.remove(i);
        end -= 1;
    }
}

/// The subcommands named on the command line, outermost first.
fn subcommand_path(command: &Command, args: &[String]) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = command;
    for arg in args.iter().skip(1) {
        if arg == "--" {
            break;
        }
        if let Some(next) = current.find_subcommand(arg) {
            if next.get_name() != "help" {
                path.push(next.get_name().to_string());
                current = next;
            }
        }
    }
    path
}

fn visible_subcommands(command: &Command) -> impl Iterator<Item = &Command> {
    command.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help")
}

fn value_name(arg: &Arg) -> String {
    match arg.get_value_names() {
        Some(names) if !names.is_empty() => names[0].to_string(),
        _ => arg.get_id().as_str().to_uppercase(),
    }
}

/// `<file> [more]...` for a command's positional arguments.
fn synopsis(command: &Command) -> String {
    let mut parts = Vec::new();
    for arg in command.get_positionals().filter(|arg| !arg.is_hide_set()) {
        let name = value_name(arg);
        let many = arg.get_num_args().is_some_and(|range| range.max_values() > 1);
        let mut part = if arg.is_required_set() { format!("<{name}>") } else { format!("[{name}]") };
        if many {
            part.push_str("...");
        }
        parts.push(part);
    }
    parts.join(" ")
}

/// `  -p, --priority <n>  What it does (default: 2)` lines for a command's options.
fn option_lines(command: &Command, indent: &str, global: bool) -> String {
    let mut text = String::new();
    for arg in command.get_arguments() {
        let id = arg.get_id().as_str();
        if arg.is_positional() || arg.is_hide_set() || id == "help" || id == "version" || arg.is_global_set() != global
        {
            continue;
        }
        let mut flags = match (arg.get_short(), arg.get_long()) {
            (Some(short), Some(long)) => format!("-{short}, --{long}"),
            (Some(short), None) => format!("-{short}"),
            (None, Some(long)) => format!("--{long}"),
            (None, None) => continue,
        };
        let takes_value = arg.get_action().takes_values();
        if takes_value {
            flags.push_str(&format!(" <{}>", value_name(arg)));
        }
        let mut help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
        let defaults: Vec<String> = arg.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect();
        if takes_value && !defaults.is_empty() {
            help.push_str(&format!(" (default: {})", defaults.join(", ")));
        }
        text.push_str(&format!("{indent}{flags}  {help}\n"));
    }
    text
}

/// One line per subcommand, each followed by its options; nested subcommands are flattened.
fn command_lines(command: &Command, prefix: &str, indent: &str) -> String {
    let mut text = String::new();
    for sub in visible_subcommands(command) {
        let name = format!("{prefix}{}", sub.get_name());
        let args = synopsis(sub);
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        let args = if args.is_empty() { String::new() } else { format!(" {args}") };
        text.push_str(&format!("{indent}{name}{args}  {about}\n"));
        text.push_str(&option_lines(sub, &format!("{indent}  "), false));
        text.push_str(&command_lines(sub, &format!("{name} "), indent));
    }
    text
}

/// The whole reference for an application in a few lines: every command with its arguments
/// and options. This is what an agent gets for `--help`.
///
/// ```text
/// todo 1.0.0 - A tiny todo list
/// usage: todo <command> [args]
/// commands:
///   add <text>  Add a task
///     -p, --priority <n>  1 (high) to 3 (low) (default: 2)
///   list  Show the tasks
/// ```
pub fn agent_help(command: &Command) -> String {
    agent_help_at(command, &[])
}

fn agent_help_at(command: &Command, path: &[String]) -> String {
    let mut root = command.clone();
    root.build();
    let app = root.get_name().to_string();
    let mut target = &root;
    for name in path {
        match target.find_subcommand(name) {
            Some(next) => target = next,
            None => break,
        }
    }
    let about = target.get_about().map(|a| a.to_string()).unwrap_or_default();
    let args = synopsis(target);
    let args = if args.is_empty() { String::new() } else { format!(" {args}") };
    let has_commands = visible_subcommands(target).next().is_some();
    let mut text = String::new();

    if path.is_empty() {
        let version = root.get_version().map(|v| format!(" {v}")).unwrap_or_default();
        let about = if about.is_empty() { String::new() } else { format!(" - {about}") };
        text.push_str(&format!("{app}{version}{about}\n"));
        if has_commands {
            text.push_str(&format!("usage: {app} <command> [args]\ncommands:\n"));
            text.push_str(&command_lines(target, "", "  "));
        } else if !args.is_empty() {
            text.push_str(&format!("usage: {app}{args}\n"));
        }
        let options = format!("{}{}", option_lines(target, "  ", false), option_lines(target, "  ", true));
        if !options.is_empty() {
            text.push_str("options:\n");
            text.push_str(&options);
        }
    } else {
        text.push_str(&format!("{app} {}{args}  {about}\n", path.join(" ")));
        text.push_str(&option_lines(target, "  ", false));
        text.push_str(&option_lines(target, "  ", true));
        text.push_str(&command_lines(target, &format!("{} ", path.join(" ")), "  "));
    }
    text
}

/// What could come next on a command line: a command first, then that command's options.
fn complete(command: &Command, before: &str) -> Vec<String> {
    let text = before.trim_start_matches(' ');
    let names = |command: &Command| visible_subcommands(command).map(|c| c.get_name().to_string()).collect::<Vec<_>>();
    let mut words: Vec<&str> = text.split(' ').collect();
    let typing = words.pop().unwrap_or_default();
    if words.is_empty() {
        return names(command).into_iter().chain(["help".to_string(), "exit".to_string()]).collect();
    }
    if words[0] == "help" {
        return names(command);
    }
    // Follow the commands that were typed, as deep as they go: `remote add` is the command
    // `add` inside `remote`.
    let (mut current, mut depth) = (command, 0);
    for word in &words {
        match current.find_subcommand(word) {
            Some(sub) => (current, depth) = (sub, depth + 1),
            None => break,
        }
    }
    if !typing.starts_with('-') {
        // After commands only, the commands inside the last one; after anything else, nothing.
        return if depth == words.len() { names(current) } else { Vec::new() };
    }
    let mut found = Vec::new();
    if depth > 0 {
        // The command's own options first, then the global ones.
        for global in [false, true] {
            for arg in current.get_arguments() {
                let id = arg.get_id().as_str();
                if arg.is_positional() || arg.is_hide_set() || id == "help" || id == "version" {
                    continue;
                }
                if arg.is_global_set() != global {
                    continue;
                }
                match (arg.get_long(), arg.get_short()) {
                    (Some(long), _) => found.push(format!("--{long}")),
                    (None, Some(short)) => found.push(format!("-{short}")),
                    (None, None) => {}
                }
            }
        }
    }
    found.push("--help".to_string());
    found
}

type More<'a> = Box<dyn Fn(&str) -> Completion + 'a>;

/// What [`Shell::before`] decides about a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// Run this as a command line: the line as it was typed, or another in its place.
    Run(String),
    /// Nothing more to do: the line was dealt with, or is not to be run.
    Done,
}

/// A prompt that reads command lines and runs them: [`interactive`], with the parts a program
/// may want to make its own.
///
/// ```no_run
/// # use clap::Parser;
/// # #[derive(Parser)] enum Todo { List }
/// # fn run(_: Todo) {}
/// use clikit::clap::{Line, Shell};
/// use clikit::Candidate;
///
/// let folder = std::cell::RefCell::new(String::from("/"));
/// Shell::<Todo>::new()
///     .prompt(|| format!("todo {}> ", folder.borrow()))
///     .history_file(".todo_history")
///     .complete(|_before| vec![Candidate::new("cd", "Go to another folder")])
///     .before(|line| match line.strip_prefix("cd ") {
///         Some(to) => {
///             *folder.borrow_mut() = to.to_string();
///             Line::Done
///         }
///         None => Line::Run(line.to_string()),
///     })
///     .run(run);
/// ```
pub struct Shell<'a, T: Parser> {
    prompt: Option<Box<dyn Fn() -> String + 'a>>,
    before: Box<dyn FnMut(&str) -> Line + 'a>,
    complete: More<'a>,
    history: Option<std::path::PathBuf>,
    parser: std::marker::PhantomData<T>,
}

impl<T: Parser> Default for Shell<'_, T> {
    fn default() -> Self {
        Shell::new()
    }
}

impl<'a, T: Parser> Shell<'a, T> {
    pub fn new() -> Shell<'a, T> {
        Shell {
            prompt: None,
            before: Box::new(|line| Line::Run(line.to_string())),
            complete: Box::new(|_| Completion::default()),
            history: None,
            parser: std::marker::PhantomData,
        }
    }

    /// What stands in front of each line; asked anew every time, so it can show where the
    /// program is. The default is the program's name and `> `.
    pub fn prompt(mut self, prompt: impl Fn() -> String + 'a) -> Shell<'a, T> {
        self.prompt = Some(Box::new(prompt));
        self
    }

    /// See every line before it is run. Return [`Line::Done`] for one you dealt with (a command
    /// that exists only at the prompt) or that should not run (the person said no), and
    /// [`Line::Run`] with the line, or with another to run in its place.
    pub fn before(mut self, before: impl FnMut(&str) -> Line + 'a) -> Shell<'a, T> {
        self.before = Box::new(before);
        self
    }

    /// More for Tab to offer, next to the program's commands and options: the function gets
    /// the text before the cursor, as for [`clikit::Editor::completer`](crate::Editor::completer).
    /// A [`Completion`] that says itself which part of the line it replaces stands alone: the
    /// program's commands and options are then not offered with it.
    pub fn complete<C: Into<Completion>>(mut self, complete: impl Fn(&str) -> C + 'a) -> Shell<'a, T> {
        self.complete = Box::new(move |before| complete(before).into());
        self
    }

    /// Keep the history of the prompt in this file, from one run of the program to the next.
    pub fn history_file(mut self, path: impl Into<std::path::PathBuf>) -> Shell<'a, T> {
        self.history = Some(path.into());
        self
    }

    /// Read lines and hand each parsed command to `run`, until `exit`, `quit`, the end of the
    /// input, or Ctrl-C on an empty line. Ctrl-C on a line with text drops the text.
    pub fn run(mut self, mut run: impl FnMut(T)) {
        let mut command = T::command();
        command.build();
        let app = command.get_name().to_string();
        let t = ctx::theme();
        let live = crate::prompt::keys_available();
        let fixed = format!("{}{app}{}> ", t.c(CYAN), t.c(RESET));
        let more = self.complete;
        let mut editor = Editor::new().completer(|before| {
            let mut completion = more(before);
            if completion.from.is_none() {
                let own = complete(&command, before).into_iter().map(Candidate::from);
                completion.candidates = own.chain(completion.candidates).collect();
            }
            completion
        });
        if let Some(path) = self.history.take() {
            editor = editor.history_file(path);
        }
        if live {
            crate::hint(format!(
                "{app} interactive: type a command, 'help' or 'exit'. Tab completes, an empty line opens a menu."
            ));
        }
        loop {
            let prompt = self.prompt.as_ref().map_or_else(|| fixed.clone(), |prompt| prompt());
            let Read::Line(line) = editor.read(&prompt) else { break };
            let mut line = line.trim().to_string();
            if !line.is_empty() {
                line = match (self.before)(&line) {
                    Line::Run(line) => line.trim().to_string(),
                    Line::Done => continue,
                };
            }
            match line.as_str() {
                "exit" | "quit" | "q" => break,
                "?" => line = "help".to_string(),
                "" => {
                    if !live {
                        continue;
                    }
                    let width = visible_subcommands(&command).map(|c| c.get_name().chars().count()).max().unwrap_or(0);
                    let entries: Vec<String> = visible_subcommands(&command)
                        .map(|c| {
                            format!(
                                "{:width$}  {}",
                                c.get_name(),
                                c.get_about().map(|a| a.to_string()).unwrap_or_default()
                            )
                        })
                        .collect();
                    match crate::select("Run a command", &entries, None) {
                        Ok(entry) => line = entry.split(' ').next().unwrap_or_default().to_string(),
                        Err(_) => continue,
                    }
                }
                _ => {}
            }
            let words = match crate::split_words(&line) {
                Ok(words) => words,
                Err(problem) => {
                    msg(Level::Error, &format!("cannot parse: {line} ({problem})"));
                    continue;
                }
            };
            let args = std::iter::once(app.clone()).chain(words);
            // `--agent`, `--plain` and `--no-color` on a line are for that line.
            let _line = ctx::mode_guard();
            if let Ok(parsed) = try_parse_from::<T>(args) {
                run(parsed);
            }
        }
    }
}

/// A prompt that reads command lines and runs them until `exit`, `quit` or the end of input.
///
/// At a terminal, Tab and Shift-Tab step through the commands, and after a command through
/// its options or the commands inside it; up and down recall earlier lines; an empty line
/// opens a menu of the commands; Ctrl-C drops the line, and on an empty line leaves the prompt.
/// Piped input works too, one command per line. A line that does not parse prints clap's
/// error and the prompt continues. [`Shell`] is the same with a prompt, a history file,
/// completion and a first look at each line of your own.
///
/// ```no_run
/// # use clap::Parser;
/// # #[derive(Parser)] enum Todo { List }
/// # fn run(_: Todo) {}
/// clikit::clap::interactive::<Todo>(run);
/// ```
pub fn interactive<T: Parser>(run: impl FnMut(T)) {
    Shell::<T>::new().run(run);
}
