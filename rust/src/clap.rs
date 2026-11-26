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
use crate::edit::{edit, Edited, Kind};
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
    let commands = || visible_subcommands(command).map(|c| c.get_name().to_string());
    if !text.contains(' ') {
        return commands().chain(["help".to_string(), "exit".to_string()]).collect();
    }
    let first = text.split(' ').next().unwrap_or_default();
    if first == "help" {
        return commands().collect();
    }
    if !text.rsplit(' ').next().unwrap_or_default().starts_with('-') {
        return Vec::new(); // after a command only options can be completed
    }
    let mut found = Vec::new();
    if let Some(sub) = command.find_subcommand(first) {
        // The command's own options first, then the global ones.
        for global in [false, true] {
            for arg in sub.get_arguments() {
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

/// A prompt that reads command lines and runs them until `exit`, `quit` or the end of input.
///
/// At a terminal, Tab and Shift-Tab step through the commands, and after a command through
/// its options; up and down recall earlier lines; an empty line opens a menu of the commands.
/// Piped input works too, one command per line. A line that does not parse prints clap's
/// error and the prompt continues.
///
/// ```no_run
/// # use clap::Parser;
/// # #[derive(Parser)] enum Todo { List }
/// # fn run(_: Todo) {}
/// clikit::clap::interactive::<Todo>(run);
/// ```
pub fn interactive<T: Parser>(mut run: impl FnMut(T)) {
    let mut command = T::command();
    command.build();
    let app = command.get_name().to_string();
    let t = ctx::theme();
    let live = crate::prompt::keys_available();
    let prompt = format!("{}{app}{}> ", t.c(CYAN), t.c(RESET));
    let mut history: Vec<String> = Vec::new();
    if live {
        crate::hint(format!(
            "{app} interactive: type a command, 'help' or 'exit'. Tab completes, an empty line opens a menu."
        ));
    }
    loop {
        let line = if live {
            match edit(&prompt, Kind::Line, &|before| complete(&command, before), &history) {
                Edited::Text(line) => line,
                _ => break,
            }
        } else {
            match ctx::read_line() {
                Some(line) => line,
                None => break,
            }
        };
        if live && !line.trim().is_empty() {
            history.push(line.clone());
        }
        let mut line = line.trim().to_string();
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
                        format!("{:width$}  {}", c.get_name(), c.get_about().map(|a| a.to_string()).unwrap_or_default())
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
        if let Ok(parsed) = try_parse_from::<T>(args) {
            run(parsed);
        }
    }
}
