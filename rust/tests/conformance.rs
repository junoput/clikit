//! Runs the shared conformance cases (`../conformance/cases.txt`) against this crate.
//! The shell implementation runs the same file; see `SPEC.md` for the format.

use std::process::Command;

use clikit::testing::Session;
use clikit::{Mode, Progress, PromptError, Task, Tasks};

/// What lasts from one call of a case to the next: the progress bar, and the group of tasks
/// with its tasks by name.
#[derive(Default)]
struct Running {
    bar: Option<Progress>,
    group: Option<(Tasks, Vec<(String, Task)>)>,
}

impl Running {
    /// The task of that name. A name not seen before joins the group, and without a group
    /// one called "Total" starts, as in the shell.
    fn task(&mut self, name: &str) -> Task {
        let (tasks, named) = self.group.get_or_insert_with(|| (Tasks::new("Total"), Vec::new()));
        if let Some((_, task)) = named.iter().find(|(known, _)| known == name) {
            return task.clone();
        }
        let task = tasks.add(name, 1);
        named.push((name.to_string(), task.clone()));
        task
    }
}

#[derive(Default)]
struct Case {
    line: usize,
    mode: String,
    name: String,
    env: Vec<(String, String)>,
    calls: Vec<String>,
    stdin: String,
    out: String,
    err: String,
    status: i32,
}

/// `\e` `\t` `\r` `\s` `\b` are escapes; a trailing `\c` means "no newline after this line".
fn decode(text: &str) -> String {
    let text = text.replace("\\e", "\x1b").replace("\\t", "\t").replace("\\r", "\r").replace("\\s", " ");
    let text = text.replace("\\b", "\x7f");
    match text.strip_suffix("\\c") {
        Some(open) => open.to_string(),
        None => text + "\n",
    }
}

fn parse(source: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut case: Option<Case> = None;
    for (i, line) in source.lines().enumerate() {
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() || line.starts_with('[') {
            cases.extend(case.take());
        }
        if let Some(rest) = line.strip_prefix('[') {
            let (mode, name) = rest.split_once("] ").expect("[mode] name");
            case = Some(Case { line: i + 1, mode: mode.into(), name: name.into(), ..Case::default() });
            continue;
        }
        let Some(case) = case.as_mut() else { continue };
        if let Some(pair) = line.strip_prefix("% ") {
            let (name, value) = pair.split_once('=').expect("% NAME=value");
            case.env.push((name.into(), value.into()));
        } else if let Some(call) = line.strip_prefix("$ ") {
            case.calls.push(call.into());
        } else if let Some(input) = line.strip_prefix('<') {
            case.stdin.push_str(&decode(input.strip_prefix(' ').unwrap_or(input)));
        } else if let Some(text) = line.strip_prefix('|') {
            case.out.push_str(&decode(text));
        } else if let Some(text) = line.strip_prefix('!') {
            case.err.push_str(&decode(text));
        } else if let Some(status) = line.strip_prefix("= ") {
            case.status = status.parse().expect("= N");
        }
    }
    cases.extend(case);
    cases
}

/// Replace every duration (`12ms`, `4.2s`, `3s`) with `<T>`.
fn without_durations(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let mut end = j;
            if end + 1 < chars.len() && chars[end] == '.' && chars[end + 1].is_ascii_digit() {
                end += 2;
            }
            if end < chars.len() && chars[end] == 'm' {
                end += 1;
            }
            if end < chars.len() && chars[end] == 's' {
                result.push_str("<T>");
                i = end + 1;
                continue;
            }
            result.extend(&chars[i..j]);
            i = j;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn command(words: &[String]) -> Command {
    let mut command = Command::new(&words[0]);
    command.args(&words[1..]);
    command
}

fn status_of(result: std::io::Result<std::process::ExitStatus>) -> i32 {
    result.map(clikit::exit_code).unwrap_or(127)
}

fn prompt_status<T>(result: Result<T, PromptError>, print: impl FnOnce(T)) -> i32 {
    match result {
        Ok(answer) => {
            print(answer);
            0
        }
        Err(PromptError::NoOptions) => 2,
        Err(PromptError::Cancelled) => 3,
        Err(_) => 1,
    }
}

/// Leading `FLAG VALUE` pairs, then the rest.
fn values<'a>(flag: &str, args: &'a [String]) -> (Vec<&'a str>, &'a [String]) {
    let mut found = Vec::new();
    let mut rest = args;
    while rest.len() >= 2 && rest[0] == flag {
        found.push(rest[1].as_str());
        rest = &rest[2..];
    }
    (found, rest)
}

/// One call from a case: the function name as the shell spells it (without `cli_`), then its
/// arguments. Returns the exit status the shell function would give.
fn call(words: &[String], running: &mut Running) -> i32 {
    let bar = &mut running.bar;
    let (name, args) = (words[0].as_str(), &words[1..]);
    let text = args.join(" ");
    match name {
        "info" => clikit::info(text),
        "ok" | "success" => clikit::ok(text),
        "warn" => clikit::warn(text),
        "error" => clikit::error(text),
        "debug" => clikit::debug(text),
        "hint" => clikit::hint(text),
        "steps" => clikit::steps(args[0].parse().unwrap()),
        "step" => clikit::step(text),
        "exec" => return status_of(clikit::exec(&mut command(args))),
        "style" => println_out(&clikit::style(&args[0], args[1..].join(" "))),
        "header" => clikit::header(text),
        "hr" => clikit::hr(),
        "nl" => clikit::nl(),
        "box" => match args {
            [flag, title, lines @ ..] if flag == "-t" => clikit::boxed(Some(title), lines),
            lines => clikit::boxed(None, lines),
        },
        "list" => match args {
            [flag, items @ ..] if flag == "-n" => clikit::list_numbered(items),
            items => clikit::list(items),
        },
        "kv" => {
            let pairs: Vec<(&String, &String)> = args.chunks_exact(2).map(|pair| (&pair[0], &pair[1])).collect();
            clikit::kv(&pairs);
        }
        "table" => {
            let (mut table, mut separator, mut rows) = (clikit::Table::new(), "|", args);
            loop {
                match rows {
                    [flag, value, rest @ ..] if flag == "-s" => (separator, rows) = (value.as_str(), rest),
                    [flag, value, rest @ ..] if flag == "-a" => (table, rows) = (table.align(value), rest),
                    [flag, rest @ ..] if flag == "-b" => (table, rows) = (table.border(true), rest),
                    [flag, rest @ ..] if flag == "-H" => (table, rows) = (table.header(false), rest),
                    _ => break,
                }
            }
            let rows: Vec<Vec<&str>> = rows.iter().map(|row| row.split(separator).collect()).collect();
            table.print(&rows);
        }
        "timeline" => {
            let (mut timeline, mut separator, mut rows) = (clikit::Timeline::new(), "|", args);
            loop {
                match rows {
                    [flag, value, rest @ ..] if flag == "-s" => (separator, rows) = (value.as_str(), rest),
                    [flag, rest @ ..] if flag == "-v" => (timeline, rows) = (timeline.vertical(true), rest),
                    [flag, rest @ ..] if flag == "-r" => (timeline, rows) = (timeline.newest_first(true), rest),
                    _ => break,
                }
            }
            let rows: Vec<Vec<&str>> = rows.iter().map(|row| row.split(separator).collect()).collect();
            timeline.print(&rows);
        }
        "tree" => {
            let (mut tree, mut separator, mut dir, mut rows) = (clikit::Tree::new(), "|", None, args);
            loop {
                match rows {
                    [flag, value, rest @ ..] if flag == "-s" => (separator, rows) = (value.as_str(), rest),
                    [flag, value, rest @ ..] if flag == "-d" => {
                        (tree, rows) = (tree.depth(value.parse().unwrap_or(0)), rest)
                    }
                    [flag, value, rest @ ..] if flag == "-p" => (dir, rows) = (Some(value), rest),
                    [flag, rest @ ..] if flag == "-a" => (tree, rows) = (tree.hidden(true), rest),
                    _ => break,
                }
            }
            if let Some(dir) = dir {
                return i32::from(tree.print_dir(dir).is_err());
            }
            let rows: Vec<Vec<&str>> = rows.iter().map(|row| row.split(separator).collect()).collect();
            tree.print(&rows);
        }
        "browse" => {
            let (mut browse, mut separator, mut list, mut rest) = (clikit::Browse::new(), "|", false, args);
            loop {
                match rest {
                    [flag, value, more @ ..] if flag == "-s" => (separator, rest) = (value.as_str(), more),
                    [flag, more @ ..] if flag == "-l" => (list, rest) = (true, more),
                    [flag, more @ ..] if flag == "-a" => (browse, rest) = (browse.hidden(true), more),
                    [flag, more @ ..] if flag == "-d" => (browse, rest) = (browse.directories(true), more),
                    _ => break,
                }
            }
            if list {
                let rows: Vec<Vec<&str>> = rest[1..].iter().map(|row| row.split(separator).collect()).collect();
                return prompt_status(browse.ask_rows(&rest[0], &rows), |answer| println_out(&answer));
            }
            let dir = rest.get(1).map(String::as_str).unwrap_or(".");
            return prompt_status(browse.ask(&rest[0], dir), |answer| println_out(&answer.display().to_string()));
        }
        "duration" => println_out(&clikit::duration(args[0].parse().unwrap())),
        "time" => return clikit::time(&args[0], || status_of(command(&args[1..]).status())),
        "countdown" => clikit::countdown(args[0].parse().unwrap(), &args[1]),
        "progress" => {
            let label = args.get(2).map(String::as_str).unwrap_or("");
            let active = bar.get_or_insert_with(|| Progress::new(args[1].parse().unwrap(), label));
            active.set(args[0].parse().unwrap());
            if active.is_done() {
                *bar = None;
            }
        }
        "progress_done" => *bar = None,
        "tasks" => {
            if let Some((earlier, _)) = running.group.take() {
                earlier.finish();
            }
            let (separator, rows) = match args {
                [flag, value, rest @ ..] if flag == "-s" => (value.as_str(), rest),
                rest => ("|", rest),
            };
            let tasks = Tasks::new(rows.first().map_or("Total", String::as_str));
            let mut named = Vec::new();
            for row in rows.iter().skip(1) {
                let mut cells = row.split(separator).map(|cell| cell.trim_matches(' '));
                let name = cells.next().unwrap_or_default();
                let weight = cells.next().filter(|weight| weight.bytes().all(|b| b.is_ascii_digit()));
                if !name.is_empty() {
                    named.push((name.to_string(), tasks.add(name, weight.and_then(|w| w.parse().ok()).unwrap_or(1))));
                }
            }
            running.group = Some((tasks, named));
        }
        "task" => match args {
            [name, current, total] => running.task(name).set(current.parse().unwrap(), total.parse().unwrap()),
            [name] => running.task(name).start(),
            _ => panic!("task NAME [CURRENT TOTAL]"),
        },
        "task_done" => running.task(&args[0]).done(),
        "task_fail" => running.task(&args[0]).fail(args.get(1).map_or("", String::as_str)),
        "tasks_done" => {
            if let Some((tasks, _)) = running.group.take() {
                return i32::from(!tasks.finish());
            }
        }
        "spin" => return status_of(clikit::spin_command(&args[0], &mut command(&args[1..]))),
        "select" => {
            let (found, rest) = values("-d", args);
            let result = clikit::select(&rest[0], &rest[1..], found.first().copied());
            return prompt_status(result, |answer| println_out(&answer));
        }
        "multiselect" => {
            let (found, rest) = values("-d", args);
            let result = clikit::multiselect(&rest[0], &rest[1..], &found);
            return prompt_status(result, |answers| answers.iter().for_each(|answer| println_out(answer)));
        }
        "confirm" => {
            // `-p` prints yes or no instead of answering with the exit status.
            let (print, args) = match args {
                [flag, rest @ ..] if flag == "-p" => (true, rest),
                rest => (false, rest),
            };
            let default = args.get(1).map(|answer| answer.starts_with(['y', 'Y']));
            return match clikit::confirm(&args[0], default) {
                Ok(answer) if print => {
                    println_out(if answer { "yes" } else { "no" });
                    0
                }
                Ok(true) => 0,
                Ok(false) => 1,
                Err(PromptError::Cancelled) => 3,
                Err(_) => 2,
            };
        }
        "input" => {
            let (suggestions, rest) = values("-c", args);
            let result = clikit::input_with(&rest[0], rest.get(1).map(String::as_str), &suggestions);
            return prompt_status(result, |answer| println_out(&answer));
        }
        "password" => return prompt_status(clikit::password(&args[0]), |answer| println_out(&answer)),
        "pause" => clikit::pause(args.first().map(String::as_str)),
        "mode" => println_out(clikit::mode().as_str()),
        "is_agent" => return i32::from(!clikit::is_agent()),
        "has" => return i32::from(!clikit::has(&args[0])),
        other => panic!("the conformance driver does not know the function '{other}'"),
    }
    0
}

/// Content printed by the driver itself goes through the crate, so that it is captured.
fn println_out(text: &str) {
    clikit::testing::print(&format!("{text}\n"));
}

#[test]
fn conformance() {
    // The tree and browse cases read `conformance/files`, by that relative name.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../conformance")).expect("conformance/");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../conformance/cases.txt");
    let cases = parse(&std::fs::read_to_string(path).expect("conformance/cases.txt"));
    assert!(cases.len() > 100, "only {} cases were found", cases.len());
    let mut failures = Vec::new();
    for case in &cases {
        let mode = Mode::parse(&case.mode).expect("a known mode");
        let mut session = Session::new(mode).env("TERM", "xterm").stdin(&case.stdin);
        for (name, value) in &case.env {
            session = session.env(name, value);
        }
        let mut status = 0;
        let run = session.run(|| {
            let mut running = Running::default();
            for line in &case.calls {
                let words = clikit::split_words(line).expect("a well-formed call");
                status = call(&words, &mut running);
            }
        });
        let timed = case.out.contains("<T>") || case.err.contains("<T>");
        let (out, err) =
            if timed { (without_durations(&run.out), without_durations(&run.err)) } else { (run.out, run.err) };
        if out != case.out || err != case.err || status != case.status {
            failures.push(format!(
                "line {}: [{}] {}\n  status {} (expected {})\n  stdout {:?}\n  wanted {:?}\n  stderr {:?}\n  wanted {:?}",
                case.line, case.mode, case.name, status, case.status, out, case.out, err, case.err
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases failed:\n\n{}", failures.len(), cases.len(), failures.join("\n\n"));
    println!("{} conformance cases passed", cases.len());
}
