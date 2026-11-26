//! Good-looking command-line output that adapts to whoever is reading it.
//!
//! | Mode | When | What you get |
//! |---|---|---|
//! | rich | standard output and error are a terminal | colours, symbols, animation, key-driven prompts |
//! | plain | pipes, files, CI | the same words, no escape codes, no animation |
//! | agent | `CLI_AGENT=1`, or an agent harness is detected | only what is needed, in as few tokens as possible |
//!
//! Status lines ([`info`], [`ok`], progress, prompts) go to standard error. Content ([`list`],
//! [`table`], [`timeline`], [`tree`], [`header`]) goes to standard output.
//!
//! ```
//! clikit::steps(2);
//! clikit::step("Build");
//! let built: Result<(), String> = clikit::spin("Compiling", || Ok(()));
//! clikit::step("Report");
//! clikit::table(&[["File", "Size"], ["app", "4.2 MB"]]);
//! clikit::ok("Done");
//! # let _ = built;
//! ```
//!
//! The same contract is implemented for shell scripts (`sh/cli.sh` in this repository) and
//! checked against the same cases (`conformance/cases.txt`). `SPEC.md` describes it.
//!
//! With the `clap` feature, [`clap`] adds agent-mode help pages, the `--agent`, `--plain` and
//! `--no-color` flags, and an interactive prompt to a clap application.

mod ctx;
mod edit;
mod keys;
mod layout;
mod msg;
mod progress;
mod prompt;
mod tasks;
mod text;
mod time;
mod timeline;
mod tree;

pub mod testing;

#[cfg(feature = "clap")]
pub mod clap;

pub use ctx::{auto_mode, color, is_agent, mode, mode_guard, set_color, set_mode, unicode, width, Mode, ModeGuard};
pub use layout::{boxed, header, hr, kv, list, list_numbered, nl, style, table, Table};
pub use msg::{debug, die, error, hint, info, ok, step, steps, warn};
pub use progress::{exec, exit_code, has, require, spin, spin_command, Progress};
pub use prompt::{confirm, input, input_with, multiselect, password, pause, select, wizard, PromptError};
pub use tasks::{Task, Tasks};
pub use text::{size, split_words};
pub use time::{countdown, duration, time, Timer};
pub use timeline::{timeline, Timeline};
pub use tree::{browse, tree, tree_dir, Browse, Tree};
