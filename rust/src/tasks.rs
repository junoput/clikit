//! Several tasks under one total.

use std::fmt::Display;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use crate::ctx::{self, Mode, BOLD, CYAN, DIM, GREEN, RED, RESET};
use crate::layout::fit;
use crate::msg::{msg, Level};
use crate::text::len;
use crate::time::since;

const BAR_WIDTH: isize = 24;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Running,
    Done,
    Failed,
}

struct Item {
    name: String,
    weight: u64,
    state: State,
    current: u64,
    /// 0 when the task is not counted.
    total: u64,
    start: Option<Instant>,
    /// How long it took, or why it failed.
    text: String,
}

struct Group {
    title: String,
    start: Instant,
    items: Vec<Item>,
    /// The lines on the screen that the next drawing replaces.
    drawn: usize,
    frame: usize,
    quarter: u64,
    done: usize,
    failed: usize,
    /// A task has started or ended.
    used: bool,
    ended: bool,
}

impl Group {
    fn run(&mut self, id: usize, current: u64, total: u64) {
        self.used = true;
        let item = &mut self.items[id];
        match item.state {
            State::Done | State::Failed => return, // what has ended stays ended
            State::Waiting => (item.start, item.state) = (Some(Instant::now()), State::Running),
            State::Running => {}
        }
        item.current = if total > 0 { current.min(total) } else { current };
        item.total = total;
        if total > 0 && current >= total {
            self.end(id, State::Done, "");
        } else {
            self.draw(false);
        }
    }

    fn end(&mut self, id: usize, state: State, why: &str) {
        let mode = ctx::theme().mode;
        self.used = true;
        let item = &mut self.items[id];
        if matches!(item.state, State::Done | State::Failed) {
            return;
        }
        let took = since(item.start.unwrap_or_else(Instant::now));
        item.state = state;
        if state == State::Done {
            if mode == Mode::Plain {
                msg(Level::Ok, &format!("{} ({took})", item.name));
            }
            item.text = took;
        } else {
            if mode != Mode::Rich {
                let reason = if why.is_empty() { " failed".to_string() } else { format!(": {why}") };
                msg(Level::Error, &format!("{}{reason}", item.name));
            }
            item.text = why.to_string();
        }
        self.draw(false);
    }

    /// Show the group as it stands; at the end, what stays on the screen.
    fn draw(&mut self, end: bool) {
        let t = ctx::theme();
        let n = self.items.len();
        let (mut waiting, mut sum, mut weight) = (0, 0, 0);
        let mut name_width = len(&self.title);
        let mut count_width = n.to_string().len() * 2 + 8;
        (self.done, self.failed) = (0, 0);
        for item in &self.items {
            let mut points = (item.current * 1000).checked_div(item.total).unwrap_or(0);
            match item.state {
                State::Done => (points, self.done) = (1000, self.done + 1),
                State::Failed => self.failed += 1,
                State::Waiting => (points, waiting) = (0, waiting + 1),
                State::Running => {}
            }
            weight += item.weight;
            sum += item.weight * points;
            name_width = name_width.max(len(&item.name));
            count_width = count_width.max(item.total.to_string().len() * 2 + 1);
        }
        let weight = weight.max(1);
        let percent = sum / (weight * 10);
        let (done, title) = (self.done, &self.title);
        match t.mode {
            Mode::Agent => return,
            Mode::Plain => {
                // A line each time the total passes a quarter.
                let quarter = percent / 25;
                if !end && quarter > self.quarter && quarter < 4 {
                    self.quarter = quarter;
                    ctx::err(&format!("{title}: {percent}% ({done}/{n})\n"));
                }
                return;
            }
            Mode::Rich => {}
        }

        let width = ctx::width();
        name_width = name_width.min((width / 3).max(8));
        let bar_width = (width as isize - name_width as isize - 12 - count_width as isize).clamp(4, BAR_WIDTH) as usize;
        let page =
            ctx::env("CLI_PAGE").and_then(|page| page.parse::<usize>().ok()).filter(|page| *page > 0).unwrap_or(10);
        // Too many to list: only the ones running or failed.
        let many = n > page;
        let (cyan, bold, dim, green, red, reset) = (t.c(CYAN), t.c(BOLD), t.c(DIM), t.c(GREEN), t.c(RED), t.c(RESET));
        let ellipsis = t.g("…", "~");
        let frames: Vec<char> = t.g("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "|/-\\").chars().collect();
        let frame = if end { t.g("·", ".").to_string() } else { frames[self.frame % frames.len()].to_string() };
        self.frame += 1;
        let bar = |fill: u64| {
            let fill = fill as usize;
            format!(
                "{green}{}{reset}{dim}{}{reset}",
                t.g("█", "#").repeat(fill),
                t.g("░", "-").repeat(bar_width - fill)
            )
        };
        let (mut out, mut lines) = (String::new(), 0);
        for item in &self.items {
            if many && matches!(item.state, State::Waiting | State::Done) {
                continue;
            }
            let name = fit(&item.name, name_width, 'l', ellipsis);
            let line = match item.state {
                State::Waiting => format!("{dim}{} {}{reset}", t.g("·", "."), name.trim_end_matches(' ')),
                State::Done => format!("{green}{}{reset} {name}  {dim}{}{reset}", t.g("✓", "+"), item.text),
                State::Failed => {
                    let why = if item.text.is_empty() { "failed" } else { &item.text };
                    format!("{red}{}{reset} {name}  {dim}{why}{reset}", t.g("✗", "x"))
                }
                State::Running if item.total > 0 => {
                    let (current, total) = (item.current, item.total);
                    let percent = current * 100 / total;
                    let bar = bar(current * bar_width as u64 / total);
                    format!("{cyan}{frame}{reset} {name}  {bar} {percent:>3}%  {dim}{current}/{total}{reset}")
                }
                State::Running => {
                    let running = since(item.start.unwrap_or_else(Instant::now));
                    format!("{cyan}{frame}{reset} {name}  {dim}{running}{reset}")
                }
            };
            out.push_str(&format!("{line}\x1b[K\n"));
            lines += 1;
        }
        if !end {
            if many {
                out.push_str(&format!("  {dim}{done} done, {waiting} waiting{reset}\x1b[K\n"));
                lines += 1;
            }
            let name = fit(title, name_width, 'l', ellipsis);
            let bar = bar(sum * bar_width as u64 / (weight * 1000));
            let running = since(self.start);
            out.push_str(&format!(
                "  {bold}{name}{reset}  {bar} {percent:>3}%  {dim}{done}/{n} {running}{reset}\x1b[K\n"
            ));
            lines += 1;
        }
        let up = if self.drawn > 0 { format!("\x1b[{}A", self.drawn) } else { String::new() };
        ctx::err(&format!("{up}{out}\x1b[J"));
        self.drawn = lines;
    }

    fn finish(&mut self) -> bool {
        if self.ended {
            return self.failed == 0;
        }
        self.ended = true;
        self.draw(true);
        let (title, done, n, took) = (&self.title, self.done, self.items.len(), since(self.start));
        if ctx::theme().mode == Mode::Agent {
            ctx::err(&format!("{title}: {done}/{n} {took}\n"));
        } else {
            let level = match (self.failed, done < n) {
                (0, false) => Level::Ok,
                (0, true) => Level::Info,
                _ => Level::Error,
            };
            msg(level, &format!("{title} ({done}/{n}, {took})"));
        }
        self.failed == 0
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        // A group that never started anything was never shown: it leaves without a word.
        if self.used {
            self.finish();
        }
    }
}

fn locked(group: &Mutex<Group>) -> MutexGuard<'_, Group> {
    group.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Several tasks under one total.
///
/// Run one after the other, the tasks are the stages of a job: finished ones turn into a
/// `✓ name 1.2s` line, the next ones wait, and the last line is the bar of the whole. Run at the
/// same time they are bars stacked over that total. A task's weight says how much of the total
/// it is.
///
/// A terminal gets the live block, which ends as one line per task and one for the whole; a log
/// gets a line per finished task and one each time the total passes a quarter; an agent gets
/// the failures and one line at the end. The group ends with [`Tasks::finish`], or when it and
/// its tasks are dropped (a group in which nothing ever started is dropped silently).
///
/// ```
/// let backup = clikit::Tasks::new("Backup");
/// let (scan, copy) = (backup.add("Scan", 1), backup.add("Copy", 8));
/// scan.start();
/// scan.done();
/// for file in 1..=3 {
///     copy.set(file, 3);
/// }
/// assert!(backup.finish());
/// ```
///
/// Tasks can be handed to threads:
///
/// ```
/// let backup = clikit::Tasks::new("Backup");
/// let workers: Vec<_> = ["iPhone", "iPad"]
///     .into_iter()
///     .map(|device| {
///         let task = backup.add(device, 1);
///         std::thread::spawn(move || (1..=10).for_each(|step| task.set(step, 10)))
///     })
///     .collect();
/// workers.into_iter().for_each(|worker| worker.join().unwrap());
/// backup.finish();
/// ```
#[derive(Clone)]
pub struct Tasks {
    group: Arc<Mutex<Group>>,
}

impl Tasks {
    /// A group with no tasks yet. `title` names the total. Nothing is printed until a task
    /// starts.
    pub fn new(title: impl Display) -> Tasks {
        let group = Group {
            title: title.to_string(),
            start: Instant::now(),
            items: Vec::new(),
            drawn: 0,
            frame: 0,
            quarter: 0,
            done: 0,
            failed: 0,
            used: false,
            ended: false,
        };
        Tasks { group: Arc::new(Mutex::new(group)) }
    }

    /// Add a task. `weight` is its share of the total: of `("Scan", 1)` and `("Copy", 8)`, the
    /// first is a ninth of the job.
    pub fn add(&self, name: impl Display, weight: u64) -> Task {
        let mut group = locked(&self.group);
        group.items.push(Item {
            name: name.to_string(),
            weight,
            state: State::Waiting,
            current: 0,
            total: 0,
            start: None,
            text: String::new(),
        });
        Task { group: self.group.clone(), id: group.items.len() - 1 }
    }

    /// End the group: the block gives way to one line for the whole. False when a task failed.
    pub fn finish(&self) -> bool {
        locked(&self.group).finish()
    }
}

/// One task of a [`Tasks`] group. It can be cloned and sent to another thread.
#[derive(Clone)]
pub struct Task {
    group: Arc<Mutex<Group>>,
    id: usize,
}

impl Task {
    /// The task is running, without a count: it shows a spinner and the time.
    pub fn start(&self) {
        locked(&self.group).run(self.id, 0, 0);
    }

    /// The task is at `current` of `total`. At `total` it is done.
    pub fn set(&self, current: u64, total: u64) {
        locked(&self.group).run(self.id, current, total);
    }

    /// The task is done.
    pub fn done(&self) {
        locked(&self.group).end(self.id, State::Done, "");
    }

    /// The task failed; `why` is shown next to it. The total stays where it is.
    pub fn fail(&self, why: impl Display) {
        locked(&self.group).end(self.id, State::Failed, &why.to_string());
    }
}
