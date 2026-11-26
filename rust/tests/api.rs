//! What the shared conformance cases cannot reach: the parts of the API that only exist here.

use clikit::testing::{capture, Session};
use clikit::{Mode, Progress, Timer};

#[test]
fn spin_reports_success_and_failure() {
    let run = capture(Mode::Agent, || {
        let fine: Result<u32, String> = clikit::spin("Thinking", || Ok(42));
        assert_eq!(fine, Ok(42));
        let broken: Result<(), String> = clikit::spin("Linking", || Err("missing symbol: main".into()));
        assert!(broken.is_err());
    });
    let lines: Vec<&str> = run.err.lines().collect();
    assert!(lines[0].starts_with("ok: Thinking ("), "{}", run.err);
    assert!(lines[1].starts_with("error: Linking failed ("), "{}", run.err);
    assert_eq!(lines[2], "missing symbol: main");
}

#[test]
fn spin_command_that_cannot_start() {
    let run = capture(Mode::Plain, || {
        let result = clikit::spin_command("Build", &mut std::process::Command::new("no-such-program-xyz"));
        assert!(result.is_err());
    });
    assert!(run.err.starts_with("[....] Build\n[ERROR] Build failed ("), "{}", run.err);
}

#[test]
fn progress_inc_and_drop() {
    let run = capture(Mode::Agent, || {
        let mut bar = Progress::new(3, "Copying");
        bar.inc();
        bar.inc();
        assert!(!bar.is_done());
    });
    assert!(run.err.starts_with("Copying: 2/3 "), "an unfinished bar reports where it stopped: {}", run.err);

    let run = capture(Mode::Plain, || {
        let mut bar = Progress::new(2, "");
        bar.inc();
        bar.inc();
        assert!(bar.is_done());
    });
    assert_eq!(run.err, "50% (1/2)\n100% (2/2)\n");

    assert_eq!(capture(Mode::Agent, || drop(Progress::new(5, "Unused"))).err, "");
}

#[test]
fn tasks_under_one_total() {
    let run = capture(Mode::Plain, || {
        let job = clikit::Tasks::new("Job");
        let (first, second) = (job.add("first", 1), job.add("second", 3));
        first.start();
        first.done();
        second.set(2, 4);
        second.fail(format_args!("exit {}", 3));
        assert!(!job.finish(), "a failed task fails the group");
        assert!(!job.finish(), "and ending twice says the same without printing again");
    });
    let lines: Vec<&str> = run.err.lines().collect();
    assert!(lines[0].starts_with("[OK] first ("), "{}", run.err);
    assert_eq!(lines[1..3], ["Job: 25% (1/2)", "Job: 62% (1/2)"]);
    assert_eq!(lines[3], "[ERROR] second: exit 3");
    assert!(lines[4].starts_with("[ERROR] Job (1/2, ") && lines.len() == 5, "{}", run.err);

    // Dropped: a group that was used reports where it stopped, one that was not says nothing.
    let run = capture(Mode::Agent, || clikit::Tasks::new("Job").add("only", 1).set(1, 2));
    assert!(run.err.starts_with("Job: 0/1 "), "{}", run.err);
    let run = capture(Mode::Agent, || drop(clikit::Tasks::new("Unused").add("only", 1)));
    assert_eq!(run.err, "");
}

#[test]
fn status_lines_and_content_go_above_what_is_redrawn() {
    let run = Session::new(Mode::Rich).env("NO_COLOR", "1").env("LANG", "en_US.UTF-8").run(|| {
        let job = clikit::Tasks::new("Job");
        job.add("a", 1).set(1, 2);
        clikit::list(&["content"]);
        job.finish();
        clikit::list(&["after"]);
    });
    // Standard output gets the content only; the block is taken away before it and put back after.
    assert_eq!(run.out, "  • content\n  • after\n");
    let block = "⠋ a    ████████████░░░░░░░░░░░░  50%  1/2\x1b[K\n";
    assert!(run.err.starts_with(block), "{:?}", run.err);
    assert_eq!(run.err.matches("\x1b[2A\x1b[J").count(), 1, "erased once, for the content: {:?}", run.err);
    assert_eq!(run.err.matches(block).count(), 2, "drawn, and drawn again under the content: {:?}", run.err);
}

#[test]
fn what_the_theme_decided_can_be_asked() {
    let utf = |mode| Session::new(mode).env("LANG", "en_US.UTF-8").env("COLUMNS", "42");
    utf(Mode::Rich).run(|| assert_eq!((clikit::color(), clikit::unicode(), clikit::width()), (true, true, 42)));
    utf(Mode::Rich).env("NO_COLOR", "1").run(|| assert_eq!((clikit::color(), clikit::unicode()), (false, true)));
    utf(Mode::Plain).run(|| assert_eq!((clikit::color(), clikit::unicode()), (false, false)));
    Session::new(Mode::Rich).env("LANG", "C").run(|| assert_eq!((clikit::color(), clikit::unicode()), (true, false)));
    Session::new(Mode::Agent).run(|| assert_eq!(clikit::width(), 80));
}

#[test]
fn sizes_and_what_a_task_shows() {
    assert_eq!([clikit::size(0), clikit::size(1023), clikit::size(1024)], ["0 B", "1023 B", "1.0 KB"]);
    assert_eq!([clikit::size(1_048_575), clikit::size(u64::MAX)], ["1.0 MB", "16384.0 PB"]);

    use std::time::Duration;
    let run =
        Session::new(Mode::Rich).env("NO_COLOR", "1").env("LANG", "en_US.UTF-8").env("COLUMNS", "100").run(|| {
            let job = clikit::Tasks::new("Copy");
            job.started(Duration::from_secs(7200));
            let photos = job.add("Photos", 1).bytes().rate().eta();
            photos.started(Duration::from_secs(1000));
            photos.set(11_744_051_200, 35_232_153_600);
            photos.note("verifying");
            job.add("Earlier", 1).done_in(Duration::from_secs(842));
        });
    let lines: Vec<&str> = run.err.lines().collect();
    assert!(lines[0].contains("33%  10.9 GB/32.8 GB  11.2 MB/s  33m "), "{}", lines[0]);
    // In its first second a task has no rate yet.
    let fresh = Session::new(Mode::Rich)
        .env("NO_COLOR", "1")
        .run(|| clikit::Tasks::new("Copy").add("Files", 1).rate().eta().set(5, 10));
    assert!(fresh.err.contains("50%  5/10\x1b[K"), "{:?}", fresh.err);
    assert!(run.err.contains(" left  verifying"), "the note comes last: {}", run.err);
    assert!(run.err.contains("✓ Earlier  14m 02s"), "{}", run.err);
    assert!(run.err.contains("● Copy (1/2, 2h 00m)"), "{}", run.err);
    assert!(run.err.contains("■ Photos"), "a task still running at the end was stopped: {}", run.err);
}

#[test]
fn tables_measure_what_a_terminal_shows() {
    let online = "\x1b[32monline\x1b[0m";
    let run = Session::new(Mode::Rich).env("NO_COLOR", "1").env("LANG", "en_US.UTF-8").env("COLUMNS", "40").run(|| {
        clikit::table(&[["Name", "State"], ["📱 Küche", online], ["日本語", "idle"]]);
        clikit::Table::new()
            .keep("Id")
            .keep("7")
            .print(&[["Name", "State", "Id"], ["A device with a long name", "online", "00008150-001A2B3C4D5E6F78"]]);
    });
    let lines: Vec<&str> = run.out.lines().collect();
    assert_eq!(
        lines[..4],
        ["  Name      State", "  ────────  ──────", "  📱 Küche  \x1b[32monline\x1b[0m", "  日本語    idle"]
    );
    assert_eq!(lines[6], "  A d…  onli…  00008150-001A2B3C4D5E6F78", "the column that is kept is whole");
}

#[test]
fn tasks_on_threads() {
    let run = capture(Mode::Agent, || {
        let job = clikit::Tasks::new("Job");
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let task = job.add(format!("worker {worker}"), 1);
                std::thread::spawn(move || (1..=50).for_each(|step| task.set(step, 50)))
            })
            .collect();
        workers.into_iter().for_each(|worker| worker.join().unwrap());
        assert!(job.finish());
    });
    assert!(run.err.starts_with("Job: 4/4 "), "{}", run.err);
}

#[test]
fn timer_and_time() {
    let timer = Timer::start();
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert!((30..2000).contains(&timer.ms()), "{}", timer.ms());
    assert!(timer.elapsed().ends_with("ms"), "{}", timer.elapsed());

    let run = capture(Mode::Plain, || assert_eq!(clikit::time("Adding", || 1 + 1), 2));
    assert!(run.err.starts_with("[INFO] Adding: "), "{}", run.err);
}

#[test]
fn display_values_are_accepted() {
    let run = capture(Mode::Agent, || {
        clikit::info(42);
        clikit::error(std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"));
        clikit::ok(format!("{} files", 3));
    });
    assert_eq!(run.err, "42\nerror: no such file\nok: 3 files\n");
}

#[test]
fn set_mode_changes_a_captured_run() {
    let run = capture(Mode::Plain, || {
        clikit::ok("a");
        clikit::set_mode(Mode::Agent);
        assert!(clikit::is_agent());
        clikit::ok("b");
    });
    assert_eq!(run.err, "[OK] a\nok: b\n");
}

#[test]
fn captures_do_not_leak_between_threads() {
    let threads: Vec<_> = [Mode::Agent, Mode::Plain]
        .into_iter()
        .map(|mode| {
            std::thread::spawn(move || (0..200).map(|_| capture(mode, || clikit::ok("x")).err).collect::<Vec<_>>())
        })
        .collect();
    let results: Vec<Vec<String>> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(results[0].iter().all(|err| err == "ok: x\n"));
    assert!(results[1].iter().all(|err| err == "[OK] x\n"));
}

#[test]
fn a_panic_ends_the_capture() {
    let result = std::panic::catch_unwind(|| capture(Mode::Agent, || panic!("boom")));
    assert!(result.is_err());
    // Not captured any more: this goes to the real stderr and the next capture starts clean.
    assert_eq!(capture(Mode::Agent, || clikit::ok("after")).err, "ok: after\n");
}

#[test]
fn split_words() {
    assert_eq!(clikit::split_words("  a  'b c' \"d \\\" e\" f\\ g ''").unwrap(), ["a", "b c", "d \" e", "f g", ""]);
    assert!(clikit::split_words("a 'b").is_err());
    assert!(clikit::split_words("").unwrap().is_empty());
}

#[test]
fn table_and_kv_take_owned_and_borrowed_text() {
    let rows = vec![vec!["Name".to_string(), "Size".to_string()], vec!["app".to_string(), "4 MB".to_string()]];
    let run = Session::new(Mode::Agent).run(|| {
        clikit::table(&rows);
        clikit::table(&[["a", "b"], ["c", "d"]]);
        clikit::kv(&[("host", "example.org".to_string())]);
        clikit::table::<[&str; 0], &str>(&[]);
    });
    assert_eq!(run.out, "Name\tSize\napp\t4 MB\na\tb\nc\td\nhost: example.org\n");
}

#[derive(Default, Debug, PartialEq)]
struct Answers {
    name: String,
    fruit: String,
    save: bool,
}

fn ask_name(a: &mut Answers) -> Result<(), clikit::PromptError> {
    a.name = clikit::input("Name", Some(&a.name))?;
    Ok(())
}

fn ask_fruit(a: &mut Answers) -> Result<(), clikit::PromptError> {
    let default = if a.fruit.is_empty() { "apple" } else { &a.fruit };
    a.fruit = clikit::select("Fruit", &["apple", "banana", "cherry"], Some(default))?;
    Ok(())
}

fn ask_save(a: &mut Answers) -> Result<(), clikit::PromptError> {
    a.save = clikit::confirm("Save", Some(true))?;
    Ok(())
}

#[test]
fn wizard_steps_back_on_escape() {
    // name; fruit: back; name: enter keeps it; fruit: banana; save: back; fruit: one further; save: yes
    let keys = "ana\n\x1b\n\n\x1b[B\n\x1b\n\x1b[B\ny";
    let mut answers = Answers::default();
    let mut result = Ok(());
    Session::new(Mode::Rich).env("CLI_INTERACTIVE", "1").stdin(keys).run(|| {
        result = clikit::wizard(&mut answers, &[&ask_name, &ask_fruit, &ask_save]);
    });
    assert_eq!(result, Ok(()));
    assert_eq!(answers, Answers { name: "ana".into(), fruit: "cherry".into(), save: true });
}

#[test]
fn wizard_ends_on_escape_at_the_first_question_and_on_other_errors() {
    let mut answers = Answers::default();
    let mut result = Ok(());
    Session::new(Mode::Rich).env("CLI_INTERACTIVE", "1").stdin("\x1b\n").run(|| {
        result = clikit::wizard(&mut answers, &[&ask_name, &ask_fruit]);
    });
    assert_eq!(result, Err(clikit::PromptError::Cancelled));

    // Without keys nothing can step back: questions are asked in order, and a missing answer ends it.
    let mut answers = Answers::default();
    let run = Session::new(Mode::Agent).stdin("bo\n").run(|| {
        result = clikit::wizard(&mut answers, &[&ask_name, &ask_fruit, &ask_save]);
    });
    assert_eq!(result, Ok(()));
    assert_eq!(answers, Answers { name: "bo".into(), fruit: "apple".into(), save: true });
    assert_eq!(run.err, "Name: bo\nFruit: apple\nSave y\n");

    let mut answers = Answers::default();
    Session::new(Mode::Agent).run(|| result = clikit::wizard(&mut answers, &[&ask_name, &ask_fruit]));
    assert_eq!(result, Err(clikit::PromptError::NoAnswer));
}

#[test]
fn table_builder() {
    let rows = [["Item", "Qty"], ["Apple", "3"], ["Melon", "12"]];
    let run = capture(Mode::Plain, || {
        clikit::table(&rows);
        clikit::Table::new().border(true).print(&rows[..2]);
        clikit::Table::new().header(false).align("rl").print(&rows[1..]);
    });
    assert_eq!(
        run.out,
        "  Item   Qty\n  -----  ---\n  Apple    3\n  Melon   12\n\
         +-------+-----+\n| Item  | Qty |\n+-------+-----+\n| Apple |   3 |\n+-------+-----+\n  \
         Apple  3\n  Melon  12\n"
    );
}

#[test]
fn timeline_takes_ragged_owned_and_borrowed_rows() {
    let owned = vec![
        vec!["2".to_string(), "b".to_string(), "y".to_string(), "a".to_string()],
        vec!["1".to_string(), "a".to_string()],
    ];
    let run = capture(Mode::Plain, || {
        clikit::Timeline::new().vertical(true).print(&owned);
        clikit::Timeline::new().vertical(true).newest_first(true).print(&[["1", "a"], ["2", "a"]]);
        clikit::timeline::<[&str; 0], &str>(&[]);
        clikit::timeline(&[["", "", ""]]);
    });
    assert_eq!(run.out, "  *    1  a\n  +-*  2  b  y\n  *  2  a\n  *  1  a\n");

    let across = Session::new(Mode::Rich).env("COLUMNS", "17").env("NO_COLOR", "1").env("LANG", "en_US.UTF-8");
    let run = across.run(|| clikit::timeline(&[vec!["9", "b"], vec!["1", "a"], vec!["5", "b", "", "a"]]));
    assert_eq!(run.out, "     1          9\n  a  ●─────╮\n  b        ●────●\n");
}

#[test]
fn editing_keys() {
    let keys = "junk\x15one twx\x17two three\x17\x01<\x05>!!\x1b[D\x1b[D\x0b\n";
    let mut answer = String::new();
    Session::new(Mode::Rich).env("CLI_INTERACTIVE", "1").stdin(keys).run(|| {
        answer = clikit::input("Text", None).unwrap();
    });
    assert_eq!(answer, "<one two >");
}

/// A directory to walk through: `top/{a/, b/inner/x, link -> ../elsewhere, Z, broken, .dot}`.
#[cfg(unix)]
fn files(name: &str) -> std::path::PathBuf {
    use std::fs;
    let root = std::env::temp_dir().join(format!("clikit-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for dir in ["top/b/inner", "top/a", "elsewhere/deep"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in ["top/b/inner/x", "top/Z", "top/.dot", "elsewhere/deep/y", "top/a/e\x1b[31mx"] {
        fs::write(root.join(file), "").unwrap();
    }
    std::os::unix::fs::symlink("../elsewhere", root.join("top/link")).unwrap();
    std::os::unix::fs::symlink("nowhere", root.join("top/broken")).unwrap();
    root
}

#[cfg(unix)]
#[test]
fn tree_of_a_directory() {
    let root = files("tree");
    let top = root.join("top");
    let run = capture(Mode::Plain, || {
        clikit::tree_dir(&top).unwrap();
        clikit::Tree::new().depth(1).hidden(true).print_dir(top.join("b/")).unwrap();
        assert!(clikit::tree_dir(top.join("Z")).is_err());
    });
    let (top, sub) = (top.display(), "|-- a/\n|   `-- e?[31mx\n|-- b/\n|   `-- inner/\n|       `-- x\n");
    // Directories first, a link is shown and not followed, an escape code in a name is defused.
    assert_eq!(run.out, format!("{top}/\n{sub}|-- link/ ...\n|-- Z\n`-- broken\n{top}/b/\n`-- inner/ ...\n"));
    assert_eq!(run.err, format!("[ERROR] tree: not a directory: {top}/Z\n"));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn browse_walks_through_files() {
    use clikit::{Browse, PromptError};
    let root = files("browse");
    let top = root.join("top");
    let keys = |keys: &str| Session::new(Mode::Rich).env("CLI_INTERACTIVE", "1").stdin(keys);
    keys("jjjljlj\n").run(|| assert_eq!(clikit::browse("Pick", &top), Ok(top.join("link/deep/y"))));
    keys("jjjj\n").run(|| assert_eq!(Browse::new().hidden(true).ask("Pick", &top), Ok(top.join(".dot"))));
    keys(".jjjj\n").run(|| assert_eq!(clikit::browse("Pick", &top), Ok(top.join(".dot"))));
    // Left on the directory itself shows the one above it.
    keys("hk\n").run(|| assert_eq!(Browse::new().directories(true).ask("Pick", top.join("b")), Ok(top.join("a"))));
    keys("j\x1b\n").run(|| assert_eq!(clikit::browse("Pick", &top), Err(PromptError::Cancelled)));
    Session::new(Mode::Agent).stdin("Z\n").run(|| assert_eq!(clikit::browse("Pick", &top), Ok(top.join("Z"))));
    Session::new(Mode::Agent).run(|| assert_eq!(clikit::browse("Pick", &top), Err(PromptError::NoAnswer)));
    Session::new(Mode::Agent).run(|| assert_eq!(clikit::browse("Pick", top.join("Z")), Err(PromptError::NoOptions)));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tree_and_browse_take_ragged_owned_and_borrowed_rows() {
    let owned = vec![vec!["a/b".to_string(), "note".to_string()], vec!["c".to_string()]];
    let run = capture(Mode::Agent, || {
        clikit::tree(&owned);
        clikit::tree(&[["a/b"], ["c"]]);
        clikit::tree::<[&str; 1], &str>(&[]);
    });
    assert_eq!(run.out, "a/\n b\tnote\nc\na/\n b\nc\n");
    let picked = Session::new(Mode::Agent).stdin("a/b\n");
    picked.run(|| assert_eq!(clikit::Browse::new().ask_rows("Pick", &owned), Ok("a/b".to_string())));
}
