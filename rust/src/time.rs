//! Durations, stopwatches and waiting.

use std::fmt::Display;
use std::time::{Duration, Instant};

use crate::ctx::{self, Mode, BOLD, RESET};
use crate::msg::{msg, Level};

/// Milliseconds as text: `340ms`, `4.2s`, `3m 07s`, `1h 02m`.
pub fn duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{}.{}s", ms / 1000, ms % 1000 / 100)
    } else {
        let s = ms / 1000;
        if s < 3600 {
            format!("{}m {:02}s", s / 60, s % 60)
        } else {
            format!("{}h {:02}m", s / 3600, s % 3600 / 60)
        }
    }
}

pub(crate) fn since(start: Instant) -> String {
    duration(start.elapsed().as_millis() as u64)
}

/// A stopwatch.
///
/// ```
/// let timer = clikit::Timer::start();
/// // ... work ...
/// clikit::ok(format!("Built in {}", timer.elapsed()));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Timer(Instant);

impl Timer {
    /// Start counting now.
    pub fn start() -> Timer {
        Timer(Instant::now())
    }

    /// Milliseconds since the start.
    pub fn ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }

    /// The time since the start, formatted by [`duration`].
    pub fn elapsed(&self) -> String {
        since(self.0)
    }
}

/// Run `work` and report how long it took.
pub fn time<R>(label: impl Display, work: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let result = work();
    msg(Level::Info, &format!("{label}: {}", since(start)));
    result
}

/// Wait, showing the seconds left. Without a terminal it only waits.
pub fn countdown(seconds: u64, label: impl Display) {
    let t = ctx::theme();
    if t.mode != Mode::Rich {
        std::thread::sleep(Duration::from_secs(seconds));
        return;
    }
    for left in (1..=seconds).rev() {
        ctx::err(&format!("\r{label} {}{left}s{}\x1b[K", t.c(BOLD), t.c(RESET)));
        std::thread::sleep(Duration::from_secs(1));
    }
    ctx::err("\r\x1b[K");
}
