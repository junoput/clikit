//! Several tasks under one total.

use std::fmt::Display;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::ctx::{self, Live, Mode, BOLD, CYAN, DIM, GREEN, RED, RESET, YELLOW};
use crate::layout::fit;
use crate::msg::{msg, Level};
use crate::text::{len, size, truncate};
use crate::time::duration;

const BAR_WIDTH: isize = 24;
/// At a terminal, a task that only moved on is not drawn more often than this.
const REDRAW: Duration = Duration::from_millis(80);

/// When to draw: at once, at once and for the last time, or soon (not too often).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Draw {
    Now,
    Soon,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Running,
    Done,
    Failed,
}

/// A moment something started: when it was noted, and how long before that it had begun.
#[derive(Clone, Copy)]
struct Clock {
    at: Instant,
    before: Duration,
}

impl Clock {
    fn started(before: Duration) -> Clock {
        Clock { at: Instant::now(), before }
    }

    fn ms(&self) -> u64 {
        (self.at.elapsed() + self.before).as_millis() as u64
    }
}

/// What a running task shows after its bar.
#[derive(Clone, Copy, Default)]
struct Show {
    bytes: bool,
    percent: bool,
    rate: bool,
    eta: bool,
}

struct Item {
    name: String,
    weight: u64,
    state: State,
    current: u64,
    /// 0 when the task is not counted.
    total: u64,
    start: Option<Clock>,
    /// How long before it was first reported the task began, if that is known.
    ago: Duration,
    /// How long it took, or why it failed.
    text: String,
    show: Show,
    note: String,
}

struct Group {
    title: String,
    start: Clock,
    items: Vec<Item>,
    /// The lines on the screen that the next drawing replaces.
    drawn: usize,
    /// When the block was last drawn.
    shown: Option<Instant>,
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
        let started = item.state == State::Waiting;
        match item.state {
            State::Done | State::Failed => return, // what has ended stays ended
            State::Waiting => (item.start, item.state) = (Some(Clock::started(item.ago)), State::Running),
            State::Running => {}
        }
        item.current = if total > 0 { current.min(total) } else { current };
        item.total = total;
        if total > 0 && current >= total {
            self.end(id, State::Done, "", None);
        } else {
            self.draw(if started { Draw::Now } else { Draw::Soon });
        }
    }

    fn end(&mut self, id: usize, state: State, why: &str, took: Option<Duration>) {
        let mode = ctx::theme().mode;
        self.used = true;
        let item = &mut self.items[id];
        if matches!(item.state, State::Done | State::Failed) {
            return;
        }
        let took = match took {
            Some(took) => duration(took.as_millis() as u64),
            None => duration(item.start.map_or(0, |start| start.ms())),
        };
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
        self.draw(Draw::Now);
    }

    /// Show the group as it stands; at the end, what stays on the screen.
    fn draw(&mut self, how: Draw) {
        let end = how == Draw::End;
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
            // Room after the bar for what the task shows, so that the bars keep their length
            // while it changes.
            let mut room = match item.show {
                Show { percent: true, .. } => 0,
                Show { bytes: true, .. } => 10 + len(&size(item.total)),
                _ => item.total.to_string().len() * 2 + 1,
            };
            if item.state == State::Running {
                room += if item.show.rate { 13 } else { 0 } + if item.show.eta { 13 } else { 0 };
                room += if item.note.is_empty() { 0 } else { 2 + len(&item.note) };
            }
            count_width = count_width.max(room);
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
        // A backup of fifty thousand files would otherwise draw fifty thousand times. Without a
        // terminal (a test, a recording) every step is drawn.
        if how == Draw::Soon && ctx::stderr_tty() && self.shown.is_some_and(|at| at.elapsed() < REDRAW) {
            return;
        }
        self.shown = Some(Instant::now());

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
        // A task still running when the group ends was stopped, and says so.
        let lead = if end {
            format!("{}{}{reset}", t.c(YELLOW), t.g("■", "="))
        } else {
            format!("{cyan}{}{reset}", frames[self.frame % frames.len()])
        };
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
                State::Running => {
                    let (current, total, show) = (item.current, item.total, item.show);
                    let elapsed = item.start.map_or(0, |start| start.ms());
                    let mut line = format!("{lead} {name}");
                    let mut tail: Vec<String> = Vec::new();
                    let room;
                    // A task that is counted has a total to divide by.
                    if let Some(filled) = (current * bar_width as u64).checked_div(total) {
                        let bar = bar(filled);
                        line.push_str(&format!("  {bar} {:>3}%", current * 100 / total));
                        room = width as isize - name_width as isize - bar_width as isize - 12;
                        match show {
                            Show { percent: true, .. } => {}
                            Show { bytes: true, .. } => tail.push(format!("{}/{}", size(current), size(total))),
                            _ => tail.push(format!("{current}/{total}")),
                        }
                        // How fast, and how long still: from the average so far.
                        if elapsed >= 1000 && current > 0 {
                            if show.rate {
                                let rate = current * 1000 / elapsed;
                                let rate = match (show.bytes, rate < 10) {
                                    (true, _) => size(rate),
                                    (false, true) => {
                                        let tenths = current * 10000 / elapsed;
                                        format!("{}.{}", tenths / 10, tenths % 10)
                                    }
                                    (false, false) => rate.to_string(),
                                };
                                tail.push(format!("{rate}/s"));
                            }
                            if show.eta {
                                let seconds = (total - current) * (elapsed / 1000) / current;
                                let left = if seconds < 60 { format!("{seconds}s") } else { duration(seconds * 1000) };
                                tail.push(format!("{left} left"));
                            }
                        }
                    } else {
                        tail.push(duration(elapsed));
                        room = width as isize - name_width as isize - 5;
                    }
                    if !item.note.is_empty() {
                        tail.push(item.note.clone());
                    }
                    let mut tail = tail.join("  ");
                    let room = room.max(1) as usize;
                    if len(&tail) > room {
                        tail = format!("{}{ellipsis}", truncate(&tail, room - 1));
                    }
                    if !tail.is_empty() {
                        line.push_str(&format!("  {dim}{tail}{reset}"));
                    }
                    line
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
            let running = duration(self.start.ms());
            out.push_str(&format!(
                "  {bold}{name}{reset}  {bar} {percent:>3}%  {dim}{done}/{n} {running}{reset}\x1b[K\n"
            ));
            lines += 1;
        }
        let up = if self.drawn > 0 { format!("\x1b[{}A", self.drawn) } else { String::new() };
        let text = format!("{out}\x1b[J");
        let live = Live { clear: format!("\x1b[{lines}A\x1b[J"), text: text.clone() };
        ctx::live(&format!("{up}{text}"), if end || lines == 0 { None } else { Some(live) });
        self.drawn = lines;
    }

    fn finish(&mut self) -> bool {
        if self.ended {
            return self.failed == 0;
        }
        self.ended = true;
        self.draw(Draw::End);
        let (title, done, n, took) = (&self.title, self.done, self.items.len(), duration(self.start.ms()));
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
            start: Clock::started(Duration::ZERO),
            items: Vec::new(),
            drawn: 0,
            shown: None,
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
            ago: Duration::ZERO,
            text: String::new(),
            show: Show::default(),
            note: String::new(),
        });
        Task { group: self.group.clone(), id: group.items.len() - 1 }
    }

    /// The group began `ago`: for joining work that is already running. The time shown for the
    /// whole counts from then.
    pub fn started(&self, ago: Duration) {
        locked(&self.group).start = Clock::started(ago);
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
        locked(&self.group).end(self.id, State::Done, "", None);
    }

    /// The task is done and took `took`: for a task that ended before anybody was watching.
    pub fn done_in(&self, took: Duration) {
        locked(&self.group).end(self.id, State::Done, "", Some(took));
    }

    /// The task failed; `why` is shown next to it. The total stays where it is.
    pub fn fail(&self, why: impl Display) {
        locked(&self.group).end(self.id, State::Failed, &why.to_string(), None);
    }

    /// The task began `ago`. Its time, its rate and its time left count from then.
    pub fn started(&self, ago: Duration) {
        let mut group = locked(&self.group);
        let item = &mut group.items[self.id];
        item.ago = ago;
        if item.state == State::Running {
            item.start = Some(Clock::started(ago));
        }
    }

    /// A few words shown next to the task while it runs: what it is doing right now. An empty
    /// text takes them away.
    pub fn note(&self, text: impl Display) {
        let mut group = locked(&self.group);
        group.items[self.id].note = text.to_string();
        if group.items[self.id].state == State::Running {
            group.draw(Draw::Soon);
        }
    }

    /// Count in bytes: `28.0 GB/68.9 GB` instead of plain numbers.
    pub fn bytes(self) -> Task {
        self.show(|show| show.bytes = true)
    }

    /// Show the bar and the percentage only, for a task that knows no more than that.
    pub fn percent(self) -> Task {
        self.show(|show| show.percent = true)
    }

    /// Show how fast it goes: `11.2 MB/s` in bytes, otherwise `120/s`. The average so far.
    pub fn rate(self) -> Task {
        self.show(|show| show.rate = true)
    }

    /// Show how long it will still take at this rate: `1h 02m left`.
    pub fn eta(self) -> Task {
        self.show(|show| show.eta = true)
    }

    fn show(self, change: impl FnOnce(&mut Show)) -> Task {
        change(&mut locked(&self.group).items[self.id].show);
        self
    }
}
