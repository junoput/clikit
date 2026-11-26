//! Try every feature of clikit.
//!
//!   cargo run --features clap --example demo              a menu of sections (in a terminal)
//!   cargo run --features clap --example demo -- all       every section, top to bottom
//!   cargo run --features clap --example demo -- prompts   one section
//!   cargo run --features clap --example demo -- mock      a pretend release using most features
//!   cargo run --features clap --example demo -- mock --fail
//!   cargo run --features clap --example demo -- interactive
//!   cargo run --features clap --example demo -- --agent all     what an LLM gets
//!   cargo run --features clap --example demo -- --plain all     what a log file gets
//!
//! DEMO_FAST=1 skips the waiting.

use std::process::Command;
use std::time::Duration;

use clap::{Parser, Subcommand};
use clikit as cli;

#[derive(Parser)]
#[command(name = "demo", version, about = "Try every feature of clikit")]
struct Demo {
    #[command(subcommand)]
    section: Section,
}

#[derive(Subcommand, Clone)]
enum Section {
    /// Status lines, steps, styled text
    Messages,
    /// Headers, boxes, rules
    Layout,
    /// Lists, key/values, tables
    Lists,
    /// Events on tracks: along a time axis, or one per row
    Timeline,
    /// Trees of paths or files, and walking through one
    Tree,
    /// A line editor for a prompt of your own
    Editor,
    /// Stopwatches, countdown, durations
    Time,
    /// Progress bars, tasks under one total, spinner
    Progress,
    /// Input, selectors, confirm, password
    Prompts,
    /// A pretend release that uses most features together
    Mock {
        /// Deploy target (staging, production); skips the question
        #[arg(short, long, value_name = "name")]
        env: Option<String>,
        /// Make the tests fail, to see the error path
        #[arg(long)]
        fail: bool,
    },
    /// Every section, top to bottom
    All,
    /// A prompt that runs these sections until "exit"
    Interactive,
}

const SECTIONS: [(&str, &str); 10] = [
    ("messages", "Status lines, steps, styled text"),
    ("layout", "Headers, boxes, rules"),
    ("lists", "Lists, key/values, tables"),
    ("timeline", "Events on tracks: along a time axis, or one per row"),
    ("tree", "Trees of paths or files, and walking through one"),
    ("editor", "A line editor for a prompt of your own"),
    ("time", "Stopwatches, countdown, durations"),
    ("progress", "Progress bars, tasks under one total, spinner"),
    ("prompts", "Input, selectors, confirm, password"),
    ("mock", "A pretend release that uses most features together"),
];

fn fast() -> bool {
    std::env::var_os("DEMO_FAST").is_some()
}

/// Pretend to be busy.
fn work(ms: u64) {
    if !fast() {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

fn messages() {
    cli::header("Messages");
    cli::info("Something worth knowing");
    cli::ok("It worked");
    cli::warn("Look out");
    cli::error("It broke");
    cli::hint("A tip that only humans see");
    cli::debug("Shown only with CLI_DEBUG=1");
    cli::steps(3);
    cli::step("Fetch");
    cli::step("Build");
    cli::step("Ship");
    let _ = cli::exec(Command::new("uname").arg("-s"));
    println!("{}", cli::style("bold magenta", "Styled text"));
}

fn layout() {
    cli::header("A header");
    cli::boxed(None, &["A box with one line"]);
    cli::boxed(Some("Note"), &["Plain text in a frame.", "Agents get the text without the frame."]);
    cli::hr();
    cli::nl();
}

fn lists() {
    cli::header("Lists, key/values, tables");
    cli::list(&["apples", "bananas", "cherries"]);
    cli::nl();
    cli::list_numbered(&["first", "second", "third"]);
    cli::nl();
    cli::kv(&[("host", "example.org"), ("port", "443"), ("protocol", "https")]);
    cli::nl();
    cli::table(&[
        ["Name", "Role", "Since"],
        ["Ana", "dev", "2019"],
        ["Björn", "ops", "2021"],
        ["Chidi", "design", "2024"],
    ]);
    cli::hint("Columns that hold only numbers are aligned right:");
    cli::table(&[["Item", "Qty", "Price"], ["Apple", "3", "1.20"], ["Melon", "12", "10.00"], ["Fig", "100", "0.50"]]);
    cli::hint("Borders, and alignment per column (l, r or c):");
    cli::Table::new().border(true).align("lcr").print(&[
        ["Name", "Role", "Since"],
        ["Ana", "dev", "2019"],
        ["Björn", "ops", "2021"],
    ]);
    cli::hint("No header: just aligned columns.");
    cli::Table::new().header(false).print(&[
        ["config", "~/.config/app.toml"],
        ["cache", "~/.cache/app"],
        ["log", "/var/log/app.log"],
    ]);
    cli::hint("At a terminal, a table that is too wide has its widest column cut:");
    let long = "A long description that will not fit on one line of most terminals, so the end of it is replaced by \
                an ellipsis when a person is looking";
    cli::table(&[["File", "What it is"], ["README.md", long]]);
    cli::hint("A column can be kept whole, an identifier for one: the others are cut instead. Cells may be");
    cli::hint("coloured, and emoji and CJK text take the two columns a terminal gives them:");
    cli::Table::new().keep("Id").print(&[
        ["Device".to_string(), "State".to_string(), "Id".to_string()],
        ["📱 Kitchen tablet".to_string(), cli::style("green", "online"), "00008030-000123456789ABCD".to_string()],
        [
            "A phone with a rather long name, longer than fits here".to_string(),
            cli::style("red", "failed"),
            "00008150-001A2B3C4D5E6F78".to_string(),
        ],
        ["日本語の名前".to_string(), cli::style("dim", "never"), "00008101-000A1B2C3D4E5F60".to_string()],
    ]);
    cli::hint("Each stream is judged on its own: in \"demo lists | cat\" the tables are plain and these hints keep their look.");
}

fn time() {
    cli::header("Time");
    let total = cli::Timer::start();
    let build = cli::Timer::start();
    work(400);
    println!("build took {}", build.elapsed());
    cli::time("Sleeping", || work(300));
    cli::countdown(if fast() { 0 } else { 3 }, "Continuing in");
    for ms in [340, 4250, 187_000, 3_725_000] {
        println!("{}", cli::duration(ms));
    }
    println!("this section took {}, or {} ms", total.elapsed(), total.ms());
}

fn progress() {
    cli::header("Progress");
    let mut bar = cli::Progress::new(40, "Downloading");
    for _ in 0..40 {
        work(40);
        bar.inc();
    }
    let mut bar = cli::Progress::new(40, "Stopping early");
    for _ in 0..12 {
        work(40);
        bar.inc();
    }
    bar.done();
    cli::hint("Several tasks under one total. One after the other they are the stages of a job:");
    let backup = cli::Tasks::new("Backup");
    let (scan, photos) = (backup.add("Scan", 1), backup.add("Photos", 6));
    let (messages, verify) = (backup.add("Messages", 2), backup.add("Verify", 1));
    scan.start();
    work(600);
    scan.done();
    for file in 1..=30 {
        work(40);
        photos.set(file, 30);
    }
    for message in 1..=12 {
        work(40);
        messages.set(message, 12);
    }
    verify.start();
    work(500);
    verify.done();
    backup.finish();
    cli::hint(
        "Running at the same time, here on three threads, they are bars stacked over the total. A task can fail:",
    );
    let sync = cli::Tasks::new("Sync");
    let workers: Vec<_> = [("iPhone", 40, 40), ("iPad", 200, 25), ("Watch", 30, 12)]
        .into_iter()
        .map(|(device, total, steps): (&str, u64, u64)| {
            let task = sync.add(device, 1);
            std::thread::spawn(move || {
                for step in 1..=steps {
                    work(40);
                    if device == "Watch" && step == steps {
                        task.fail("not reachable");
                    } else {
                        task.set(step * total / steps, total);
                    }
                }
            })
        })
        .collect();
    workers.into_iter().for_each(|worker| worker.join().expect("a worker"));
    if !sync.finish() {
        cli::hint("finish() returned false: a task failed.");
    }
    cli::hint("A task can show sizes, its speed and the time left, or only a percentage. A note says what it is");
    cli::hint("doing, a task can be skipped, and times that are known can be given, for work that began earlier:");
    let restore = cli::Tasks::new("Restore");
    restore.started(Duration::from_secs(95));
    let unlock = restore.add("Unlock", 1);
    let photos = restore.add("Photos", 6).bytes().rate().eta();
    let settings = restore.add("Settings", 1).percent();
    unlock.skip();
    photos.started(Duration::from_secs(95));
    for i in 1..=20u64 {
        work(80);
        photos.set(1_073_741_824 + i * 53_687_091, 2_147_483_648);
        settings.set(i * 5, 100);
        match i {
            5 => settings.note("wallpaper"),
            12 => settings.note("accounts"),
            10 => cli::warn("A message goes above the bars, not through them"),
            _ => {}
        }
    }
    photos.done();
    photos.note("1,204 files");
    restore.finish();
    let _: Result<(), String> = cli::spin("Compiling", || {
        work(1500);
        Ok(())
    });
    let _: Result<(), String> = cli::spin("A step that fails", || Err("line 12: something is wrong".to_string()));
    let _ = cli::spin_command("A program that fails", Command::new("sh").args(["-c", "echo its output; exit 3"]));
}

#[derive(Default)]
struct Answers {
    name: String,
    branch: String,
    fruit: String,
    city: String,
    toppings: Vec<String>,
    saved: bool,
}

/// What was answered before, or the default the first time round.
fn or<'a>(earlier: &'a str, default: &'a str) -> &'a str {
    if earlier.is_empty() {
        default
    } else {
        earlier
    }
}

fn prompts() {
    use cli::PromptError;
    const CITIES: [&str; 16] = [
        "Amsterdam",
        "Berlin",
        "Copenhagen",
        "Dublin",
        "Edinburgh",
        "Florence",
        "Geneva",
        "Helsinki",
        "Istanbul",
        "Lisbon",
        "Madrid",
        "Oslo",
        "Prague",
        "Rome",
        "Stockholm",
        "Vienna",
    ];
    cli::header("Prompts");
    cli::hint("Escape goes back one question. Tab and Shift-Tab step through options and suggestions.");
    cli::hint("With no terminal (or for an agent) these read a line of stdin, or take the default.");
    let mut answers = Answers::default();
    let asked = cli::wizard(
        &mut answers,
        &[
            &|a: &mut Answers| -> Result<(), PromptError> {
                a.name = cli::input("Your name", Some(or(&a.name, "stranger")))?;
                Ok(())
            },
            &|a: &mut Answers| {
                let branches = ["main", "develop", "release", "hotfix"];
                a.branch = cli::input_with("Branch (try Tab)", Some(or(&a.branch, "main")), &branches)?;
                Ok(())
            },
            &|a: &mut Answers| {
                let fruits = ["apple", "banana", "cherry", "dragon fruit"];
                a.fruit = cli::select("Favourite fruit", &fruits, Some(or(&a.fruit, "banana")))?;
                Ok(())
            },
            &|a: &mut Answers| {
                a.city = cli::select("A long list scrolls", &CITIES, Some(or(&a.city, "Lisbon")))?;
                Ok(())
            },
            &|a: &mut Answers| {
                a.toppings = cli::multiselect("Toppings", &["nuts", "honey", "cream", "chocolate"], &["nuts"])?;
                Ok(())
            },
            &|a: &mut Answers| {
                a.saved = cli::confirm("Save this?", Some(true))?;
                Ok(())
            },
        ],
    );
    if let Err(why) = asked {
        cli::warn(format!("Left the wizard: {why}"));
    }
    let secret = cli::password("A password (typing is hidden)").unwrap_or_default();
    cli::pause(None);
    cli::kv(&[
        ("name", answers.name),
        ("branch", answers.branch),
        ("fruit", answers.fruit),
        ("city", answers.city),
        ("toppings", answers.toppings.join(", ")),
        ("saved", if answers.saved { "yes" } else { "no" }.to_string()),
        ("password length", secret.chars().count().to_string()),
    ]);
}

/// A release pipeline in which nothing real happens. Returns the exit status.
fn mock(env: Option<String>, fail: bool) -> i32 {
    cli::header("A pretend release");
    let release = cli::Timer::start();
    cli::require(&["sh"]);
    cli::kv(&[("project", "demo-app"), ("version", "2.4.0"), ("branch", "main")]);
    cli::nl();
    cli::steps(5);

    cli::step("Check the workspace");
    let _: Result<(), String> = cli::spin("Looking for uncommitted changes", || {
        work(600);
        Ok(())
    });

    cli::step("Build");
    let build = cli::Timer::start();
    let _: Result<(), String> = cli::spin("Compiling 42 files", || {
        work(1400);
        Ok(())
    });
    let build = build.elapsed();

    cli::step("Test");
    let test = cli::Timer::start();
    let report = "echo 'test parse_empty ... ok'; echo 'test parse_dates ... FAILED'; echo; \
                  echo '  expected: 2026-01-31'; echo '    actual: 2026-02-00'; echo; echo '127 passed, 1 failed'";
    let script = format!("{report}; exit {}", i32::from(fail));
    work(900);
    let tests = cli::spin_command("Running 128 tests", Command::new("sh").args(["-c", &script]));
    if !tests.is_ok_and(|status| status.success()) {
        cli::error("Release stopped: the tests failed");
        cli::hint("Run it again without --fail to see the rest.");
        return 1;
    }
    let test = test.elapsed();

    cli::step("Upload");
    let target = match env {
        Some(target) => target,
        None => match cli::select("Deploy to", &["staging", "production"], Some("staging")) {
            Ok(target) => target,
            Err(_) => {
                cli::warn("Cancelled");
                return 0;
            }
        },
    };
    let upload = cli::Timer::start();
    let mut bar = cli::Progress::new(30, format!("Uploading to {target}"));
    for _ in 0..30 {
        work(50);
        bar.inc();
    }
    let upload = upload.elapsed();

    cli::step("Publish");
    if target == "production" && !cli::confirm("Really publish to production?", Some(false)).unwrap_or(false) {
        cli::warn("Stopped before publishing");
        return 0;
    }
    cli::countdown(if fast() { 0 } else { 3 }, "Publishing in");
    let _: Result<(), String> = cli::spin("Switching traffic", || {
        work(500);
        Ok(())
    });

    cli::header("Result");
    cli::table(&[
        ["Stage", "Result", "Time"],
        ["Build", "42 files", &build],
        ["Test", "128 passed", &test],
        ["Upload", "30 files", &upload],
    ]);
    cli::nl();
    cli::boxed(
        Some("Released"),
        &[format!("demo-app 2.4.0 is live on {target}"), format!("https://{target}.example.org")],
    );
    cli::ok(format!("Finished in {}", release.elapsed()));
    0
}

fn timeline() {
    cli::header("Timelines");
    // The snapshots of three devices: when, track, label, and the track it continues or merges.
    let backups: [&[&str]; 11] = [
        &["2024-02-01", "iPhone 12", "38.7 GB"],
        &["2024-05-03", "iPhone 12", "39.1 GB"],
        &["2024-07-11", "iPad", "11.9 GB"],
        &["2025-01-15", "iPhone 12", "40.2 GB"],
        &["2025-02-02", "iPad", "12.3 GB"],
        &["2025-06-01", "iPhone 15", "continues iPhone 12", "iPhone 12"],
        &["2025-09-14", "iPhone 15", "43.0 GB"],
        &["2025-11-20", "iPad", "12.8 GB"],
        &["2026-04-02", "iPhone 15", "44.1 GB"],
        &["2026-09-14", "iPhone 15", "combined with iPad", "iPad"],
        &["2026-11-20", "iPhone 15", "44.9 GB"],
    ];
    cli::hint("One line per track, as wide as the terminal. The fourth cell draws the link between two tracks:");
    cli::timeline(&backups);
    cli::hint("Vertical: one row per event. As long as it needs to be, and it has room for the labels:");
    cli::Timeline::new().vertical(true).print(&backups);
    cli::hint("The newest on top, like a commit history:");
    cli::Timeline::new().vertical(true).newest_first(true).print(&[
        vec!["2026-11-02 09:10", "main", "Initial commit"],
        vec!["2026-11-03 14:25", "main", "Add the parser"],
        vec!["2026-11-04 10:00", "feature", "Start the new output", "main"],
        vec!["2026-11-05 16:40", "main", "Fix an off-by-one"],
        vec!["2026-11-06 11:15", "feature", "Tables"],
        vec!["2026-11-09 13:30", "main", "Merge feature", "feature"],
        vec!["2026-11-10 08:05", "main", "Release 1.1"],
    ]);
    cli::hint("A track can be anything. The axis picks years, months, days or hours:");
    cli::timeline(&[
        ["2026-06-27", "Rome"],
        ["2026-06-28", "Rome"],
        ["2026-06-28", "Rome"],
        ["2026-07-02", "Rome"],
        ["2026-07-18", "Wedding"],
        ["2026-07-19", "Wedding"],
        ["2026-08-03", "Hiking"],
        ["2026-08-09", "Hiking"],
        ["2026-08-21", "Hiking"],
    ]);
    cli::hint("When can be any text: then rows keep the order they were given in.");
    cli::timeline(&[
        vec!["v1.0", "stable"],
        vec!["v1.1", "stable"],
        vec!["v2.0-beta", "next", "", "stable"],
        vec!["v1.2", "stable"],
        vec!["v2.0", "stable", "", "next"],
    ]);
    cli::hint("Without track names it is one line:");
    cli::Timeline::new().vertical(true).print(&[
        ["09:15", "", "Breakfast"],
        ["12:40", "", "Lunch"],
        ["19:05", "", "Dinner"],
    ]);
}

fn tree() {
    use std::io::IsTerminal;
    cli::header("Trees");
    // What a backup holds: path, note.
    let files = [
        ["Photos/2024/rome.jpg", "2.1 MB"],
        ["Photos/2024/wedding.jpg", "3.4 MB"],
        ["Photos/2025/hiking.jpg", "1.8 MB"],
        ["Notes/todo.txt", "4 KB"],
        ["Notes/ideas.md", "12 KB"],
        ["Contacts.vcf", "310 KB"],
    ];
    cli::hint("Rows are [path, note]. Anything that nests: the files in a backup, settings, chapters.");
    cli::tree(&files);
    cli::hint("A depth: so many levels. A directory with more inside ends in an ellipsis:");
    cli::Tree::new().depth(1).print(&files);
    cli::hint("The files under a directory:");
    let _ = cli::tree_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    cli::hint("browse walks through a tree and returns the path that was chosen.");
    if cli::mode() != cli::Mode::Rich || !std::io::stdin().is_terminal() {
        cli::hint("Without a terminal (or for an agent) it reads the path from standard input.");
        return;
    }
    cli::hint("Right opens, left closes, enter chooses. Left on the top directory shows the one above it.");
    let file = cli::browse("Pick a file", ".").map(|path| path.display().to_string());
    cli::hint("A list of paths can be walked through too; this one chooses a directory:");
    let folder = cli::Browse::new().directories(true).ask_rows("Restore which folder", &files);
    let nothing = || "(nothing)".to_string();
    cli::kv(&[("file", file.unwrap_or_else(|_| nothing())), ("folder", folder.unwrap_or_else(|_| nothing()))]);
}

fn editor() {
    use cli::{Candidate, Completion, Editor, Read};
    use std::io::IsTerminal;
    cli::header("A prompt of your own");
    if cli::status_mode() != cli::Mode::Rich || !std::io::stdin().is_terminal() {
        cli::hint("The line editor is for a person at a terminal: run this section there.");
        return;
    }
    cli::hint(
        "Type open and a space, then Tab: a name with spaces goes in quoted, a folder leaves the cursor behind it.",
    );
    cli::hint("Up and Ctrl-R go through earlier lines, Alt-B and Alt-F move by words, Ctrl-C drops the line.");
    const FILES: [(&str, &str); 5] = [
        ("Annual report.pdf", "12 pages"),
        ("Notes/", "a folder"),
        ("Notes/Ideas for next year.md", "a draft"),
        ("Notes/todo.txt", "plain text"),
        ("budget.csv", "a table"),
    ];
    let mut editor = Editor::new().completer(|before: &str| -> Completion {
        let Some((_, typed)) = before.split_once(' ') else {
            let commands = [("open", "Show a file"), ("close", "Put it away"), ("exit", "Leave this prompt")];
            return commands.into_iter().map(Candidate::from).collect::<Vec<_>>().into();
        };
        // What is one level below what was typed; a folder is only part of the word.
        let typed = typed.trim_start_matches('"');
        FILES
            .iter()
            .filter(|(path, _)| path.starts_with(typed) && *path != typed)
            .filter(|(path, _)| !path[typed.len()..].trim_end_matches('/').contains('/'))
            .map(|(path, about)| match path.ends_with('/') {
                true => Candidate::new(*path, *about).partial(),
                false => Candidate::new(*path, *about),
            })
            .collect::<Vec<_>>()
            .into()
    });
    let prompt = format!("{} ", cli::style("cyan", "files>"));
    loop {
        match editor.read(&prompt) {
            Read::Line(line) if line.trim() == "exit" => break,
            Read::Line(line) => match cli::split_words(&line) {
                Ok(words) => cli::kv(&[("words", format!("{words:?}"))]),
                Err(problem) => cli::error(problem),
            },
            Read::Interrupted | Read::End => break,
        }
    }
}

fn run(section: Section) -> i32 {
    match section {
        Section::Messages => messages(),
        Section::Layout => layout(),
        Section::Lists => lists(),
        Section::Timeline => timeline(),
        Section::Tree => tree(),
        Section::Editor => editor(),
        Section::Time => time(),
        Section::Progress => progress(),
        Section::Prompts => prompts(),
        Section::Mock { env, fail } => return mock(env, fail),
        Section::All => {
            for section in [
                Section::Messages,
                Section::Layout,
                Section::Lists,
                Section::Timeline,
                Section::Tree,
                Section::Editor,
                Section::Time,
                Section::Progress,
                Section::Prompts,
            ] {
                run(section);
            }
            return mock(None, false);
        }
        Section::Interactive => cli::clap::interactive::<Demo>(|demo| {
            run(demo.section);
        }),
    }
    0
}

fn menu() {
    let mut entries: Vec<String> = SECTIONS.iter().map(|(name, about)| format!("{name:<12} {about}")).collect();
    entries.push(format!("{:<12} {}", "all", "Every section, top to bottom"));
    entries.push("quit".to_string());
    while let Ok(entry) = cli::select("What do you want to see?", &entries, None) {
        let name = entry.split(' ').next().unwrap_or_default();
        if name == "quit" {
            break;
        }
        if let Ok(demo) = cli::clap::try_parse_from::<Demo>(["demo", name]) {
            run(demo.section);
        }
        cli::nl();
        cli::pause(Some("Press any key for the menu"));
    }
}

fn main() {
    use std::io::IsTerminal;
    if std::env::args().len() == 1 && cli::mode() == cli::Mode::Rich && std::io::stdin().is_terminal() {
        menu();
        return;
    }
    std::process::exit(run(cli::clap::parse::<Demo>().section));
}
