#![cfg(feature = "clap")]
//! The clap integration: the agent help page must be the one the shell implementation prints
//! for the same application (`sh/examples/todo.sh`).

use clap::{CommandFactory, Parser, Subcommand};
use clikit::testing::Session;
use clikit::Mode;

#[derive(Parser, Debug, PartialEq)]
#[command(name = "todo", version = "1.0.0", about = "A tiny todo list")]
struct Todo {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Action {
    /// Add a task
    Add {
        #[arg(value_name = "text")]
        text: String,
        /// 1 (high) to 3 (low)
        #[arg(short, long, value_name = "n", default_value_t = 2)]
        priority: u8,
    },
    /// Show the tasks
    List {
        /// Include finished tasks
        #[arg(short, long)]
        all: bool,
    },
    /// Mark a task as finished
    Done {
        #[arg(value_name = "number")]
        number: Option<u32>,
    },
    /// Delete every task
    Clear,
}

const HELP: &str = "\
todo 1.0.0 - A tiny todo list
usage: todo <command> [args]
commands:
  add <text>  Add a task
    -p, --priority <n>  1 (high) to 3 (low) (default: 2)
  list  Show the tasks
    -a, --all  Include finished tasks
  done [number]  Mark a task as finished
  clear  Delete every task
";

#[test]
fn agent_help_matches_the_shell_implementation() {
    assert_eq!(clikit::clap::agent_help(&Todo::command()), HELP);
}

fn run(mode: Mode, args: &[&str]) -> (Result<Todo, i32>, String, String) {
    let mut result = Err(-1);
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let captured = Session::new(mode).run(|| result = clikit::clap::try_parse_from::<Todo>(args));
    (result, captured.out, captured.err)
}

#[test]
fn parses_like_clap() {
    let (result, out, err) = run(Mode::Agent, &["todo", "add", "Buy milk", "-p", "1"]);
    assert_eq!(result, Ok(Todo { command: Action::Add { text: "Buy milk".into(), priority: 1 } }));
    assert_eq!((out.as_str(), err.as_str()), ("", ""));
}

#[test]
fn agent_gets_the_compact_page_for_help() {
    assert_eq!(run(Mode::Agent, &["todo", "--help"]), (Err(0), HELP.to_string(), String::new()));
    assert_eq!(run(Mode::Agent, &["todo", "help"]), (Err(0), HELP.to_string(), String::new()));
    assert_eq!(run(Mode::Agent, &["todo"]), (Err(2), HELP.to_string(), String::new()));
}

#[test]
fn agent_help_for_one_command() {
    let page = "todo add <text>  Add a task\n  -p, --priority <n>  1 (high) to 3 (low) (default: 2)\n";
    assert_eq!(run(Mode::Agent, &["todo", "add", "--help"]), (Err(0), page.to_string(), String::new()));
    assert_eq!(run(Mode::Agent, &["todo", "help", "add"]), (Err(0), page.to_string(), String::new()));
}

#[test]
fn agent_errors_are_one_line() {
    let (result, out, err) = run(Mode::Agent, &["todo", "lst"]);
    assert_eq!(result, Err(2));
    assert_eq!(out, "");
    assert_eq!(err, "error: unknown command 'lst'; commands: add list done clear\n");

    let (result, _, err) = run(Mode::Agent, &["todo", "add", "x", "--nope"]);
    assert_eq!(result, Err(2));
    assert_eq!(err, "error: unexpected argument '--nope' found\n");

    let (result, _, err) = run(Mode::Agent, &["todo", "add"]);
    assert_eq!(result, Err(2));
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.starts_with("error: "), "{err}");
}

#[test]
fn agent_version() {
    assert_eq!(run(Mode::Agent, &["todo", "--version"]), (Err(0), "todo 1.0.0\n".to_string(), String::new()));
}

#[test]
fn mode_flags_are_taken_from_anywhere() {
    let mut seen = None;
    let captured = Session::new(Mode::Plain).run(|| {
        let parsed = clikit::clap::try_parse_from::<Todo>(["todo", "list", "--agent", "-a"]);
        assert_eq!(parsed, Ok(Todo { command: Action::List { all: true } }));
        seen = Some(clikit::mode());
        clikit::ok("x");
    });
    assert_eq!(seen, Some(Mode::Agent));
    assert_eq!(captured.err, "ok: x\n");
}

#[test]
fn mode_flags_after_double_dash_are_arguments() {
    let (result, _, _) = run(Mode::Plain, &["todo", "add", "--", "--agent"]);
    assert_eq!(result, Ok(Todo { command: Action::Add { text: "--agent".into(), priority: 2 } }));
}

#[test]
fn no_color_flag_keeps_the_layout() {
    let captured = Session::new(Mode::Rich).run(|| {
        let _ = clikit::clap::try_parse_from::<Todo>(["todo", "--no-color", "clear"]);
        clikit::ok("done");
    });
    assert_eq!(captured.err, "✓ done\n");
}

#[test]
fn interactive_runs_piped_commands() {
    let mut seen = Vec::new();
    let input = "list\nadd \"Buy milk\" -p 1\n\nnope\nlist -a\nexit\nclear\n";
    let captured = Session::new(Mode::Agent).stdin(input).run(|| {
        clikit::clap::interactive::<Todo>(|todo| seen.push(todo.command));
    });
    assert_eq!(
        seen,
        [Action::List { all: false }, Action::Add { text: "Buy milk".into(), priority: 1 }, Action::List { all: true },]
    );
    assert_eq!(captured.err, "error: unknown command 'nope'; commands: add list done clear\n");
}

/// The interactive prompt with keys: `CLI_INTERACTIVE=1` makes it read them from standard input.
fn typed(keys: &str) -> (Vec<Action>, String) {
    let mut seen = Vec::new();
    let captured = Session::new(Mode::Rich).env("CLI_INTERACTIVE", "1").stdin(keys).run(|| {
        clikit::clap::interactive::<Todo>(|todo| seen.push(todo.command));
    });
    (seen, captured.err)
}

#[test]
fn interactive_tab_completes_commands() {
    assert_eq!(typed("l\t\nexit\n").0, [Action::List { all: false }]);
    // Tab on an empty line steps through the commands: add, list, ...
    assert_eq!(typed("\t\t\nexit\n").0, [Action::List { all: false }]);
    // Shift-Tab starts from the end, which is "exit".
    assert_eq!(typed("\x1b[Z\nlist\n").0, []);
    assert_eq!(typed("c\t\n").0, [Action::Clear]);
}

#[test]
fn interactive_tab_completes_options() {
    assert_eq!(typed("list --a\t\nexit\n").0, [Action::List { all: true }]);
    assert_eq!(typed("list -\t\nexit\n").0, [Action::List { all: true }]);
    // Nothing to complete for an argument.
    assert_eq!(typed("add mi\t\nexit\n").0, [Action::Add { text: "mi".into(), priority: 2 }]);
}

#[test]
fn interactive_mode_flags_last_one_line() {
    let mut modes = Vec::new();
    Session::new(Mode::Plain).stdin("--agent list\nlist\n--plain list\nlist\n").run(|| {
        clikit::clap::interactive::<Todo>(|_| modes.push(clikit::mode()));
        assert_eq!(clikit::mode(), Mode::Plain);
    });
    assert_eq!(modes, [Mode::Agent, Mode::Plain, Mode::Plain, Mode::Plain]);

    // A program with its own prompt gets the same from a guard around each line.
    Session::new(Mode::Rich).run(|| {
        {
            let _line = clikit::mode_guard();
            assert!(clikit::clap::try_parse_from::<Todo>(["todo", "--agent", "--no-color", "list"]).is_ok());
            assert_eq!(clikit::mode(), Mode::Agent);
        }
        assert_eq!(clikit::mode(), Mode::Rich);
        assert_eq!(clikit::style("red", "x"), "\x1b[31mx\x1b[0m", "colours are back too");
    });
}

#[test]
fn interactive_history() {
    let add = |text: &str| Action::Add { text: text.into(), priority: 2 };
    assert_eq!(typed("add a\n\x1b[A\nexit\n").0, [add("a"), add("a")]);
    assert_eq!(typed("add a\nadd b\n\x1b[A\x1b[A\x1b[B\nexit\n").0, [add("a"), add("b"), add("b")]);
    // Down brings back what was being typed.
    assert_eq!(typed("add a\nadd draft\x1b[A\x1b[B\nexit\n").0, [add("a"), add("draft")]);
}

#[test]
fn interactive_shows_the_candidates() {
    let (_, err) = typed("\t\nexit\n");
    let plain: String =
        err.replace("\x1b[1m", "").replace("\x1b[36m", "").replace("\x1b[2m", "").replace("\x1b[0m", "");
    assert!(plain.contains("\nadd  list  done  clear  help  exit  \x1b[1A\r"), "{plain:?}");
}
