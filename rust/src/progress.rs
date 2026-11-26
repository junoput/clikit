//! A progress bar, a spinner, and running other programs.

use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ctx::{self, Mode, CYAN, DIM, GREEN, RESET};
use crate::msg::{msg, Level};
use crate::time::since;

const BAR_WIDTH: u64 = 24;
/// How much of a failed command's output an agent gets.
const AGENT_TAIL: usize = 40;

/// A progress bar.
///
/// A terminal gets a live bar that gives way to one `✓ label (30/30, 1.6s)` line when it ends,
/// a log gets a line per quarter, an agent gets one line at the end. The bar ends itself when
/// it reaches the total, or when it is dropped.
///
/// ```
/// let files = ["a", "b", "c"];
/// let mut bar = clikit::Progress::new(files.len() as u64, "Copying");
/// for _file in files {
///     bar.inc();
/// }
/// ```
pub struct Progress {
    total: u64,
    label: String,
    current: u64,
    start: Option<Instant>,
    quarter: Option<u64>,
    updates: usize,
    done: bool,
}

impl Progress {
    /// A bar that is full at `total`. Nothing is printed until the first update.
    pub fn new(total: u64, label: impl Display) -> Progress {
        Progress {
            total: total.max(1),
            label: label.to_string(),
            current: 0,
            start: None,
            quarter: None,
            updates: 0,
            done: false,
        }
    }

    /// Move the bar to `current`.
    pub fn set(&mut self, current: u64) {
        let t = ctx::theme();
        let (total, current) = (self.total, current.min(self.total));
        let start = *self.start.get_or_insert_with(Instant::now);
        self.current = current;
        self.done = false;
        let percent = current * 100 / total;
        let label = &self.label;
        match t.mode {
            Mode::Agent => {}
            Mode::Plain => {
                let quarter = percent / 25;
                if self.quarter.map_or(true, |last| quarter > last) {
                    self.quarter = Some(quarter);
                    let prefix = if label.is_empty() { String::new() } else { format!("{label}: ") };
                    ctx::err(&format!("{prefix}{percent}% ({current}/{total})\n"));
                }
            }
            Mode::Rich => {
                let fill = (current * BAR_WIDTH / total) as usize;
                let prefix = if label.is_empty() { String::new() } else { format!("{label} ") };
                let (dim, reset) = (t.c(DIM), t.c(RESET));
                // A spinner frame in front, as on every other line that is still running; it turns
                // with each update. The count is padded so that the line does not shift as it grows.
                let frames: Vec<char> = t.g("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "|/-\\").chars().collect();
                let frame = frames[self.updates % frames.len()];
                self.updates += 1;
                let digits = total.to_string().len();
                ctx::err(&format!(
                    "\r{}{frame}{reset} {prefix}{}{}{reset}{dim}{}{reset} {percent:>3}% {dim}{current:>digits$}/{total} {}{reset}\x1b[K",
                    t.c(CYAN),
                    t.c(GREEN),
                    t.g("█", "#").repeat(fill),
                    t.g("░", "-").repeat(BAR_WIDTH as usize - fill),
                    since(start),
                ));
            }
        }
        if current >= total {
            self.done();
        }
    }

    /// Move the bar one forward.
    pub fn inc(&mut self) {
        self.set(self.current + 1);
    }

    /// True once the bar has ended.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// End the bar early. A full bar ends itself.
    pub fn done(&mut self) {
        let Some(start) = self.start.take() else { return };
        self.quarter = None;
        self.updates = 0;
        self.done = true;
        let (label, current, total, took) = (&self.label, self.current, self.total, since(start));
        match ctx::theme().mode {
            Mode::Agent => {
                let prefix = if label.is_empty() { String::new() } else { format!("{label}: ") };
                ctx::err(&format!("{prefix}{current}/{total} {took}\n"));
            }
            Mode::Plain => {}
            Mode::Rich => {
                // The bar gives way to one line, shaped like the result of a spinner.
                let text = if label.is_empty() {
                    format!("{current}/{total} ({took})")
                } else {
                    format!("{label} ({current}/{total}, {took})")
                };
                ctx::err("\r\x1b[K");
                msg(if current < total { Level::Info } else { Level::Ok }, &text);
            }
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.done();
    }
}

/// Animates on standard error until dropped. Only in rich mode on a real terminal.
struct Spinner {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    fn start(label: &str) -> Spinner {
        let stop = Arc::new(AtomicBool::new(false));
        let t = ctx::theme();
        if t.mode != Mode::Rich || ctx::in_scope() {
            return Spinner { stop, thread: None };
        }
        let frames: Vec<char> = t.g("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "|/-\\").chars().collect();
        let (label, flag) = (label.to_string(), stop.clone());
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let mut i = 0;
            while !flag.load(Ordering::Relaxed) {
                ctx::err(&format!(
                    "\r{}{}{} {label} {}{}s{}\x1b[K",
                    t.c(CYAN),
                    frames[i % frames.len()],
                    t.c(RESET),
                    t.c(DIM),
                    start.elapsed().as_secs(),
                    t.c(RESET),
                ));
                i += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        Spinner { stop, thread: Some(thread) }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
            ctx::err("\r\x1b[K");
        }
    }
}

fn announce(label: &str) {
    if ctx::theme().mode == Mode::Plain && !ctx::flag("CLI_QUIET") {
        ctx::err(&format!("[....] {label}\n"));
    }
}

/// Show a spinner while `work` runs, then a tick or a cross with the time it took.
///
/// The error of a failed run is shown under the cross. Anything `work` prints itself is not
/// hidden; to run another program quietly, use [`spin_command`].
///
/// ```
/// let answer: Result<u32, String> = clikit::spin("Thinking", || Ok(42));
/// assert_eq!(answer, Ok(42));
/// ```
pub fn spin<T, E: Display>(label: impl Display, work: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let label = label.to_string();
    let start = Instant::now();
    announce(&label);
    let spinner = Spinner::start(&label);
    let result = work();
    drop(spinner);
    match &result {
        Ok(_) => msg(Level::Ok, &format!("{label} ({})", since(start))),
        Err(error) => {
            msg(Level::Error, &format!("{label} failed ({})", since(start)));
            show_output(&error.to_string());
        }
    }
    result
}

/// Run a program behind a spinner. Its output is hidden unless it fails; then a person sees
/// all of it and an agent sees the last 40 lines.
///
/// ```no_run
/// use std::process::Command;
/// let status = clikit::spin_command("Compiling", Command::new("cargo").arg("build"))?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn spin_command(label: impl Display, command: &mut Command) -> io::Result<ExitStatus> {
    let label = label.to_string();
    let start = Instant::now();
    let (path, file) = temp_file()?;
    announce(&label);
    let spinner = Spinner::start(&label);
    inherit_mode(command);
    let status = command.stdin(Stdio::null()).stdout(Stdio::from(file.try_clone()?)).stderr(Stdio::from(file)).status();
    drop(spinner);
    let output = fs::read(&path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
    let _ = fs::remove_file(&path);
    match &status {
        Ok(status) if status.success() => msg(Level::Ok, &format!("{label} ({})", since(start))),
        Ok(status) => {
            msg(Level::Error, &format!("{label} failed (exit {}, {})", exit_code(*status), since(start)));
            show_output(&output);
        }
        Err(error) => msg(Level::Error, &format!("{label} failed ({error})")),
    }
    status
}

/// Show the command line, then run the program with its output passed through.
pub fn exec(command: &mut Command) -> io::Result<ExitStatus> {
    let t = ctx::theme();
    if !ctx::flag("CLI_QUIET") {
        let mut line = command.get_program().to_string_lossy().into_owned();
        for arg in command.get_args() {
            line.push(' ');
            line.push_str(&arg.to_string_lossy());
        }
        ctx::err(&format!("{}$ {line}{}\n", t.c(DIM), t.c(RESET)));
    }
    inherit_mode(command);
    if ctx::in_scope() {
        // A captured run captures the child's output too.
        let output = command.stdin(Stdio::null()).output()?;
        ctx::out(&String::from_utf8_lossy(&output.stdout));
        ctx::err(&String::from_utf8_lossy(&output.stderr));
        return Ok(output.status);
    }
    command.status()
}

/// The exit code as a shell reports it: 128 + the signal number for a killed process.
pub fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

/// A mode forced with `set_mode` (for example by `--agent`) carries over to child processes.
fn inherit_mode(command: &mut Command) {
    if let Some(mode) = ctx::forced_mode() {
        command.env("CLI_MODE", mode.as_str());
    }
}

fn show_output(output: &str) {
    if output.is_empty() {
        return;
    }
    let lines: Vec<&str> = output.lines().collect();
    let mut text = String::new();
    if ctx::theme().mode == Mode::Agent {
        if lines.len() > AGENT_TAIL {
            text.push_str(&format!("(last {AGENT_TAIL} of {} lines)\n", lines.len()));
        }
        for line in &lines[lines.len().saturating_sub(AGENT_TAIL)..] {
            text.push_str(&format!("{line}\n"));
        }
    } else {
        for line in &lines {
            text.push_str(&format!("  {line}\n"));
        }
    }
    ctx::err(&text);
}

fn temp_file() -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    for _ in 0..16 {
        let name = format!("cli.{}.{nanos}.{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join(name);
        match OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not create a temporary file"))
}

/// True when `program` is installed (found on `PATH` and executable).
pub fn has(program: &str) -> bool {
    let is_program = |path: &std::path::Path| {
        let Ok(meta) = fs::metadata(path) else { return false };
        if !meta.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    };
    if program.contains(std::path::MAIN_SEPARATOR) {
        return is_program(std::path::Path::new(program));
    }
    let Some(paths) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&paths)
        .any(|dir| is_program(&dir.join(program)) || (cfg!(windows) && is_program(&dir.join(format!("{program}.exe")))))
}

/// Exit with status 127 unless every program is installed.
pub fn require(programs: &[&str]) {
    let missing: Vec<&str> = programs.iter().copied().filter(|program| !has(program)).collect();
    if !missing.is_empty() {
        crate::msg::die(format!("missing required command(s): {}", missing.join(" ")), 127);
    }
}
