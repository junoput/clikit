//! Capture what a piece of code prints, for tests.
//!
//! A captured run has its own mode, environment and standard input, and touches none of the
//! real ones, so tests can run in parallel. Only the calling thread is captured.
//!
//! ```
//! use clikit::testing::Session;
//! use clikit::Mode;
//!
//! let run = Session::new(Mode::Agent).run(|| {
//!     clikit::ok("Built");
//!     clikit::list(&["a", "b"]);
//! });
//! assert_eq!(run.err, "ok: Built\n");
//! assert_eq!(run.out, "- a\n- b\n");
//! ```

use std::collections::HashMap;

use crate::ctx::{self, Mode, Scope};

/// What a captured run printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captured {
    /// Standard output: content.
    pub out: String,
    /// Standard error: status lines and prompts.
    pub err: String,
}

/// The setting for a captured run.
pub struct Session {
    mode: Mode,
    env: HashMap<String, String>,
    stdin: String,
}

impl Session {
    /// A run in `mode`, 80 columns wide, with a UTF-8 locale, no other environment variables
    /// and empty standard input.
    pub fn new(mode: Mode) -> Session {
        let mut env = HashMap::new();
        env.insert("COLUMNS".to_string(), "80".to_string());
        env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
        Session { mode, env, stdin: String::new() }
    }

    /// Set an environment variable for the run, such as `NO_COLOR` or `CLI_QUIET`.
    pub fn env(mut self, name: &str, value: &str) -> Session {
        self.env.insert(name.to_string(), value.to_string());
        self
    }

    /// What prompts will read. With `CLI_INTERACTIVE=1` these are key presses.
    pub fn stdin(mut self, text: &str) -> Session {
        self.stdin.push_str(text);
        self
    }

    /// Run `work` and return what it printed.
    pub fn run(self, work: impl FnOnce()) -> Captured {
        struct Leave;
        impl Drop for Leave {
            fn drop(&mut self) {
                ctx::leave_scope();
            }
        }
        ctx::enter_scope(Scope::new(self.mode, self.env, &self.stdin));
        let leave = Leave;
        work();
        std::mem::forget(leave);
        let scope = ctx::leave_scope().expect("the capture is still active");
        Captured { out: scope.out, err: scope.err }
    }
}

/// Run `work` in `mode` and return what it printed. See [`Session`] for more control.
pub fn capture(mode: Mode, work: impl FnOnce()) -> Captured {
    Session::new(mode).run(work)
}

/// Write to standard output the way the crate does, so that a captured run captures it.
/// For the output a program prints itself, next to what this crate prints for it.
pub fn print(text: &str) {
    ctx::out(text);
}
