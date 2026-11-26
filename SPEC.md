# The contract

Every implementation in this repository prints the same thing for the same call. This file says
what that is. `conformance/cases.txt` holds the exact bytes, and every implementation's tests run
against it.

Function names below are the shell ones without the `cli_` prefix. Other languages spell them the
way that language does (`cli_list -n` is `list_numbered` in Rust); each README has the table.

## Modes

Output is written for one of three readers.

| Mode | Reader | Output |
|---|---|---|
| `rich` | a person at a terminal | colours, symbols, animation, key-driven prompts |
| `plain` | a pipe, a file, CI | the same words, no escape codes, no animation |
| `agent` | an LLM | only what is needed, in as few tokens as possible |

The mode is decided once, by the first rule that applies:

1. `CLI_MODE` is `rich`, `plain` or `agent`: that mode.
2. `CLI_AGENT` is `1`, `true` or `yes`: `agent`.
3. `CLI_AGENT` is not `0`, `false` or `no`, and one of `CLAUDECODE`, `CURSOR_AGENT`, `GEMINI_CLI`,
   `CODEX_SANDBOX`, `OPENCODE`, `AI_AGENT`, `AGENT` is set: `agent`.
4. `TERM` is `dumb`: `plain`.
5. `FORCE_COLOR` is set and not `0`: `rich`.
6. Standard output and standard error are both terminals: `rich`.
7. Otherwise: `plain`.

An environment variable that is set to the empty string counts as not set.

In rich mode, colours are used unless `NO_COLOR` is set, and Unicode symbols are used when the
locale (`LC_ALL`, else `LC_CTYPE`, else `LANG`) names UTF-8. Otherwise the ASCII stand-ins are used.

| Symbol | Unicode | ASCII | | Symbol | Unicode | ASCII |
|---|---|---|---|---|---|---|
| ok | `✓` | `+` | | pointer | `❯` | `>` |
| error | `✗` | `x` | | ticked | `◉` | `[x]` |
| warning | `▲` | `!` | | not ticked | `○` | `[ ]` |
| info | `●` | `i` | | bar | `█` `░` | `#` `-` |
| debug | `·` | `.` | | rule | `─` | `-` |
| step | `▸` | `>` | | box | `╭ ╮ ╰ ╯ │` | `+ + + + \|` |
| bullet | `•` | `-` | | spinner | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | `\|/-\` |

## Streams

- **Standard error** gets status: messages, steps, progress, the spinner, and everything a prompt
  draws.
- **Standard output** gets content: headers, rules, boxes, lists, key/values, tables, help pages,
  and the answers to prompts.

So `answer=$(select ...)` captures only the answer, and `tool list | grep x` sees only the list.

## Environment

| Variable | Effect |
|---|---|
| `CLI_MODE`, `CLI_AGENT`, `NO_COLOR`, `FORCE_COLOR` | see Modes |
| `CLI_QUIET` | drop `info`, `ok`, `hint`, `step`, and the lines that announce a command |
| `CLI_DEBUG` | show `debug` lines |
| `CLI_YES` | `confirm` answers yes without asking |
| `CLI_INTERACTIVE` | `0`: never use key-driven prompts. `1`: always, reading keys from standard input |
| `CLI_PAGE` | rows a selector shows before it scrolls (default 10) |
| `COLUMNS` | the terminal width, when set |

## Output, function by function

`T` is the text passed in.

### Messages (standard error)

| Call | rich | plain | agent |
|---|---|---|---|
| `info T` | `● T` | `[INFO] T` | `T` |
| `ok T` | `✓ T` | `[OK] T` | `ok: T` |
| `warn T` | `▲ T` | `[WARN] T` | `warn: T` |
| `error T` | `✗ T` | `[ERROR] T` | `error: T` |
| `debug T` | `· T` | `[DEBUG] T` | `debug: T` |
| `hint T` | `T`, dimmed | `T` | nothing |
| `step T` after `steps 5` | `[2/5] T`, bold | `==> [2/5] T` | `[2/5] T` |
| `step T` otherwise | `▸ T`, bold | `==> T` | `T` |
| `exec CMD` | `$ CMD`, dimmed, then runs it | `$ CMD` | `$ CMD` |

In rich mode only the symbol is coloured: blue, green, yellow, red, gray.

### Layout and data (standard output)

| Call | rich | plain | agent |
|---|---|---|---|
| `header T` | blank line, `T` in bold, a rule as long as `T` | blank line, `== T ==` | `# T` |
| `hr` | a rule, 80 columns at most | the same in `-` | nothing |
| `nl` | a blank line | a blank line | nothing |
| `box` | the lines in a frame, the title in its top edge | the same in ASCII | `TITLE:` then the lines |
| `list` | `  • item` | `  - item` | `- item` |
| `list -n` | `   9. item`, numbers right-aligned | the same | `9. item` |
| `kv` | `  key:   value`, keys padded to the longest | the same | `key: value` |
| `table` | header in bold, a rule under it, columns two spaces apart | the same | rows, cells separated by a tab |
| `timeline` | a time axis, then one line per track, each in its own colour | the same in ASCII | the events in order, cells separated by a tab |
| `timeline -v` | one row per event, one column per track | the same in ASCII | the same as without `-v` |
| `tree` | the paths as a tree, directories in blue, notes dimmed in one column | the same in ASCII | one line per node, a space of indent per level, the note after a tab |

Widths count characters, so text passed to `box`, `table`, `timeline` and `tree` must not contain
escape codes.

#### Tables

Cells are trimmed of spaces and every column is as wide as its widest cell. Lines never end in
spaces.

- **Alignment.** A column is right-aligned when every non-empty cell below the header is a
  number: digits, with `.` or `,` between groups, an optional sign in front and an optional `%`
  behind (`12`, `-3`, `1,000.50`, `50%`). Everything else is left-aligned. `-a lrc` sets the
  alignment column by column (left, right, centred); columns it does not reach keep the default.
  The header is aligned like its column.
- **Borders.** `-b` draws a frame around the cells, with a line under the header.
- **No header.** `-H` treats every row as data: aligned columns and nothing else.
- **Width.** In rich mode a table wider than the terminal has its widest column shortened, one
  character at a time, until the table fits or no column is wider than four. Cut cells end in
  `…`. Plain and agent output is never cut.

An agent gets the rows tab-separated whatever the options.

#### Timelines

A row is `WHEN|TRACK|LABEL|FROM`; the last two cells are optional, cells are trimmed, and a row
whose first three cells are empty is skipped. An **event** sits at `WHEN` on the line of `TRACK`.
`FROM` names another track that this event continues or merges: a **link** between the two.

- **When.** A date is `YYYY-MM-DD`, optionally followed by `T` or a space and `HH:MM` or
  `HH:MM:SS`, optionally followed by a time zone (`Z`, `+01:00`, ` +0100`) that is read and
  ignored. A number is whole, with an optional `-` and at most 15 digits. If every row has a
  date, or every row has a number, events are sorted by it, oldest first, and events at the same
  moment keep the order they were given in. Otherwise `WHEN` is text and all rows keep their
  order.
- **Tracks** are numbered in the order they first appear in that sorted list, as a track or as a
  `FROM`. The colours are cyan, magenta, yellow, green, blue, and then again. A track's line
  runs from the first event that involves it to the last; being named in `FROM` counts.

```
across (the default)                           one row per event (-v)
             2024         2025        2026       ●      2024-02-01  iPhone 12  38.7 GB
  iPhone 12  ●─────●───────●────╮                │ ●    2024-07-11  iPad       11.9 GB
  iPad            ●─────────────┼───●──╮         ╰─┼─●  2025-06-01  iPhone 15  continues iPhone 12
  iPhone 15                     ●──────●──●        ╰─●  2026-09-14  iPhone 15  combined with iPad
                                                     ●  2026-11-20  iPhone 15  44.9 GB
```

**Across.** Two spaces, the track names padded to the longest, two spaces, then the axis. The
names take at most a third of the terminal width (never less than 8), and longer ones end in
`…`; without any name there is no name column. The axis is what is left of the terminal width,
never less than 10 columns.

- An event at position `p` is in column `round((p - min) * (columns - 1) / (max - min))`, halves
  rounded up; with one moment only, column 0. For text, `p` is the row number.
- `●` is one event, `◉` several in one column (`*` and `#` in ASCII).
- A link is a vertical line in the event's column from the `FROM` track to the event. On the
  `FROM` line it is a corner where that line starts or ends there and a tee where it goes on;
  on a track in between it is `┼` where that track's line runs and `│` where it does not.
  Events are never overwritten.
- The first line labels the axis, dimmed. The label at column 0 says where the axis starts.
  Further labels follow the range: two years or more, a year (`2025`) every 1, 2, 5, 10, 20,
  50... years; sixty days or more, a month (`2025-03`) every 1, 2, 3 or 6 months; two days or
  more, or only dates without a time, a day (`03-14`) every 1, 2, 7 or 14 days; otherwise an
  hour (`14:00`) every 1, 2, 3, 6 or 12 hours. The smallest step whose labels have room is
  taken, and a label that would come within two columns of the one before, or run off the end,
  is left out. Numbers and text get two labels: the first value at the left, the last at the
  right.

**One row per event (`-v`).** Two spaces, one column per track with a space between columns, two
spaces, then `WHEN`, the track name and the label, each padded to its longest and two spaces
apart. A column without any text is left out, and lines never end in spaces.

- The event is `●` in its track's column; a track whose line passes the row is `│`.
- A link is a horizontal line from the `FROM` column to the event, drawn the same way as across
  with the directions swapped.
- `-r` puts the newest event on top.
- In rich mode a label that does not fit the terminal is cut and ends in `…`, but never to
  less than four characters. Plain output is never cut.

**Agent.** One line per event in sorted order, whatever the options: `WHEN`, the track name
(left out when no track has a name) and the label, separated by tabs. A link adds a fourth cell
`from: TRACK`. Empty cells at the end are left out.

#### Trees

A tree comes from a list of paths or from a directory.

**From paths.** A row is `PATH|NOTE`; the note is optional, cells are trimmed, further cells are
ignored and a row without a path is skipped. The parts of a path are separated by `/`. Empty
parts and `.` are dropped, so `./a//b` is `a/b`; a path that starts with `/` keeps the slash on
its first part (`/usr`). A node is a directory when something is inside it or when a row names
it with a `/` at the end. Nodes that share a directory are shown together, in the order they
first appear. A node's note is the first one given for it.

**From a directory** (`tree -p DIR`). The first line is `DIR` as it was given, without slashes
at its end. Under it, at every level: directories first, then files, each group ordered by the
bytes of the names. A symbolic link to a directory counts as a directory and is not followed.
Names that start with a dot are left out unless `-a` is given. Control characters in a file
name are shown as `?`, so that a name cannot send escape codes to the terminal.

```
rich                          plain                       agent
src/                          src/                        src/
├── main.rs   1.2 KB          |-- main.rs   1.2 KB         main.rs<TAB>1.2 KB
╰── lib/                      `-- lib/                     lib/
    ├── x.rs                      |-- x.rs                  x.rs
    ╰── y.rs  300 B               `-- y.rs  300 B           y.rs<TAB>300 B
README.md     readme          README.md     readme        README.md<TAB>readme
```

- The nodes at the top have nothing drawn before them. Below, each level adds four columns:
  `├── ` before a node, `╰── ` before the last one in its directory, and `│   ` or four spaces
  under them.
- A directory's name ends in `/`.
- Notes start in one column: two spaces after the longest line that has a note. Lines never end
  in spaces.
- `-d N` shows `N` levels; under `DIR` for `-p`. A directory with something inside that is not
  shown ends in ` …` (` ...` outside rich mode). The same mark follows a link to a directory
  that is not empty.
- In rich mode the lines, the notes and the `…` are dimmed and directories are blue.

### Time

A duration is printed as `340ms` below a second, `4.2s` below a minute (tenths, not rounded),
`3m 07s` below an hour, then `1h 02m`.

`time LABEL CMD` runs the command and prints `info "LABEL: DURATION"`. `countdown N LABEL` redraws
`LABEL Ns` once a second in rich mode; in the other modes it only waits.

### Progress and spinner (standard error)

`progress CURRENT TOTAL [LABEL]` is called repeatedly.

- rich: one line, redrawn: a spinner frame that turns with each call, the label, a 24-cell bar,
  the percentage, `CURRENT/TOTAL` (padded to the width of the total, so the line does not shift)
  and the time so far. When it ends, the bar is replaced by `✓ LABEL (8/8, DURATION)`, the shape of
  a spinner's result, so finished work lines up whatever the labels were.
- plain: `LABEL: 50% (4/8)` each time a new quarter is reached, including the first call.
- agent: nothing until the end, then `LABEL: 8/8 DURATION`.

The bar ends when `CURRENT` reaches `TOTAL`, or when it is ended early. An early end prints
`● LABEL (2/8, DURATION)` in rich mode and `LABEL: 2/8 DURATION` in agent mode.

#### Several tasks under one total

`tasks TITLE [NAME|WEIGHT]...` names a group of tasks; nothing is shown yet. `task NAME CURRENT
TOTAL` says how far a task is, `task NAME` that it is running without a count, `task_done NAME`
and `task_fail NAME [TEXT]` how it ended, and `tasks_done` ends the group. A task is waiting,
running, done or failed. Reaching `TOTAL` makes it done; what has ended stays ended. A name not
seen before joins the group with weight 1, and a task without a group starts one called `Total`.

Tasks that run one after the other are the stages of a job; tasks that run at the same time are
bars stacked over the total. It is the same picture:

```
✓ Scan      0.8s                                   ⠹ iPhone  ██████████████████░░░░░░  75%  3/4
⠹ Photos    ████████████░░░░░░░░░░░░  50%  5/10    ⠹ iPad    █░░░░░░░░░░░░░░░░░░░░░░░   5%  10/200
· Verify                                           ✗ Watch   not reachable
  Backup    ████████████░░░░░░░░░░░░  50%  1/3 4s    Sync    ████████░░░░░░░░░░░░░░░░  35%  0/3 9s
```

**The total.** Each task has `points`: 1000 when done, `CURRENT * 1000 / TOTAL` when running or
failed and counted, otherwise 0. The percentage is `sum(WEIGHT * points) / (10 * sum(WEIGHT))`,
rounded down; `WEIGHT` is a whole number, 1 when absent. So a failed task holds the total where
it stopped.

- rich: one line per task in the order they were named, then the line of the total; each call
  redraws them all in place (cursor up, every line followed by clear-to-end-of-line, then clear
  to the end of the screen).
  - waiting: `· NAME`, dimmed. Running: a spinner frame (one frame for all, turning with each
    drawing), the name, the bar, the percentage and `CURRENT/TOTAL`; without a count, the name
    and the time so far. Done: `✓ NAME  DURATION`. Failed: `✗ NAME  TEXT`, or `failed`.
  - The total: two spaces, `TITLE` in bold, its bar and percentage, then `DONE/TASKS` and the
    time so far.
  - Names and the title are padded to the longest, cut with `…` at a third of the terminal
    width (never below 8). The bars are 24 cells, fewer when the terminal is narrow, never
    fewer than 4.
  - With more tasks than `CLI_PAGE`, waiting and finished tasks are not listed: the running and
    failed ones are, then `N done, M waiting`, then the total.
  - `tasks_done` draws the tasks once more without the total (a task still running gets `·`
    for a spinner), then `✓ TITLE (3/3, DURATION)`: `✗` when a task failed, `●` when some never
    finished.
- plain: `[OK] NAME (DURATION)` when a task is done, `[ERROR] NAME: TEXT` (or `NAME failed`)
  when it fails, `TITLE: 50% (1/3)` each time the total passes 25, 50 or 75 percent, and at the
  end `[OK] TITLE (3/3, DURATION)`, `[ERROR]` or `[INFO]` as above.
- agent: `error: NAME: TEXT` when a task fails, and at the end `TITLE: 3/3 DURATION`.

`tasks_done` has status 1 when a task failed. Naming a new group ends the one before.

**Not too often.** At a terminal (standard error is one), a call that only moves a running task
on redraws at most every 80 ms; a task that starts or ends is always drawn. Without a terminal
every call draws. The terminal's width is asked for at most once a second.

#### Status lines while something is redrawn

A status line (`info`, `ok`, `warn`, `error`, `debug`, `hint`, `step`) printed in rich mode
while a group of tasks or a progress bar is on the screen goes above it: the drawing is erased
(tasks: cursor up by its lines, clear to the end of the screen; a bar: carriage return, clear to
the end of the line), the line is printed, and the drawing is repeated as it was.

`spin LABEL CMD` runs a command with its output hidden.

- While it runs: a spinner with the label and the seconds so far (rich), `[....] LABEL` (plain),
  nothing (agent).
- When it succeeds: `ok "LABEL (DURATION)"`.
- When it fails: `error "LABEL failed (exit N, DURATION)"`, then the command's output, each line
  indented by two spaces. An agent gets the output unindented and only its last 40 lines, after
  `(last 40 of N lines)`.

### Prompts (drawn on standard error; the answer goes to standard output)

**With keys** (rich mode at a terminal, or `CLI_INTERACTIVE=1`):

- `select`: a menu. Up/down, `k`/`j` or Shift-Tab/Tab move and wrap around, Enter chooses, Escape
  cancels. The cursor starts on the default. Lists longer than `CLI_PAGE` scroll and show
  `3-12 of 40`. Keys the menu has no use for (Page Up, a letter) do nothing.
- `multiselect`: the same, with Space to tick, `a` to tick all or none. Defaults start ticked.
- When answered, the menu is replaced by `✓ QUESTION › answer`; when cancelled, by
  `✗ QUESTION (cancelled)`.
- `browse`: a tree to walk through; see below.
- `confirm`: `? QUESTION [Y/n]`, then `y`, `n`, Escape (no) or Enter (the default, if there is one).
- `input`: `? QUESTION (default)`, then a line typed in the editor described below. Escape cancels.
- `password`: the same question; what is typed is not shown. It is asked this way whenever
  standard input is a terminal, in plain mode too: in `tool export | tar x` the output is a
  pipe, and a person still types the password. Only agent mode never reads a terminal.

**The line editor** is behind `input` and behind the interactive prompt of an application.

| Key | Does |
|---|---|
| Left, Right, Home, End (also Ctrl-A, Ctrl-E) | move the cursor |
| Backspace, Delete | remove the character before, under the cursor |
| Ctrl-U, Ctrl-K, Ctrl-W | remove everything before the cursor, after it, the word before it |
| Tab, Shift-Tab | step forwards, backwards through the candidates |
| Up, Down | earlier and later lines (the interactive prompt only) |
| Enter | accept |

Tab completion works like a menu. The first Tab collects the candidates that start with what
was typed and puts the first one in its place (Shift-Tab: the last); more Tabs step through
them and wrap around. While stepping, the candidates are shown on a row under the line, the
current one highlighted; the row is cut to the terminal width and scrolls. Any other key keeps
the candidate and goes on editing. Escape puts back what was typed. A single candidate is taken
at once, and when nothing matches nothing happens.

- `input -c A -c B ...` gives the candidates for an answer. They are suggestions: anything may
  still be typed. The whole text is what gets completed.
- The interactive prompt completes the word under the cursor: a command for the first word, a
  command after `help`, and the options of the command (its own first, then global ones, then
  `--help`) for a word that starts with `-`. A completed command or option is followed by a space.

On the screen, each key redraws the line: carriage return, clear to the end of the screen, the
row of candidates if there is one (on the next line, then back up), the prompt and the text.

**Browse** shows a tree as `tree` draws it, without notes column or depth: one row per node, two
columns in front for the pointer. At first only the top is on show; directories open and close.
`browse QUESTION [DIR]` walks through the files under `DIR` (`.` when absent), whose own row is
the first and always open. `browse -l QUESTION ROW...` walks through a list of paths.

| Key | Does |
|---|---|
| Up, Down (also `k`, `j`, Shift-Tab, Tab) | move, around the ends |
| Right (also `l`) | open the directory; on an open one, step to the first row inside |
| Left (also `h`) | close the directory; on a closed one or a file, go to the directory it is in; on `DIR` itself, show the directory above with `DIR` open inside it |
| Enter | choose the file; on a directory, open or close it |
| Space | open or close the directory |
| `.` | show or hide the names that start with a dot (files only; `-a` starts with them shown) |
| Home, End | the first row, the last row |
| Escape | cancel |

- `-d` chooses a directory instead: files are not shown, and Enter takes the directory under the
  cursor, `DIR` included.
- A directory is read when it is opened, in the order described under Trees. A link to a
  directory can be opened.
- More rows than `CLI_PAGE` scroll, with `3-12 of 40` under them. A name is cut to the terminal
  width; a note is shown, two spaces after the name, only when it fits.
- The answer is the path from the top of the tree: `DIR` as it was given, then the names, joined
  by `/`, without a `./` in front. After going above `DIR` it starts with the directory that is
  then on top (`..`, `../..`, `/home`).
- When answered or cancelled, the tree is replaced by one line, as for `select`.

**Without keys**, a prompt reads one line of standard input.

- `select` and `multiselect` print the question, the options numbered from 1, and
  `choice [default]: `. The line may be an option's text or its number; text wins when an option
  is itself a number. `multiselect` takes several, separated by commas.
- An empty line or no input means the default. With no default, `select` fails; `multiselect`
  accepts an empty line as "none".
- `browse` prints `QUESTION (a file under DIR): ` and reads a path. It is taken as typed, or else
  under `DIR`, and must name a file (with `-d`: `a directory under DIR`, and a directory). With
  `-l` it prints `QUESTION (a file): ` and the line must be one of the paths in the list; a
  directory may be written with or without `/` at its end. There is no default.
- `confirm` prints `QUESTION [y/n] ` and accepts `y`, `yes`, `n`, `no`. Anything else is no answer.
- When standard input is not a terminal, the answer is echoed after the prompt, so a log reads
  `choice [staging]: production`.

**In agent mode** a prompt never blocks. It does not read from a terminal, waits at most one
second for a line, draws nothing, and on success prints one line: `QUESTION: answer`
(`QUESTION y` for `confirm`). When there is no answer and no default it fails with a line that
says what to send:

```
error: input required: Deploy to (options: staging | production). Pipe the answer on stdin.
```

Exit statuses:

| Status | Meaning |
|---|---|
| 0 | answered (`confirm`: yes) |
| 1 | no answer, a bad answer, or the input ran out (`confirm`: no) |
| 2 | `select`, `multiselect`: no options were given. `browse`: not a directory, or nothing to choose from. `confirm`: nobody answered |
| 3 | Escape |

`confirm -p` prints `yes` or `no` on standard output like the other prompts, with status 0 for
either answer.

**A wizard** asks questions in order. Escape (status 3) on a question goes back to the one
before; on the first question it ends the wizard with status 3. Any other failure ends the wizard
with that status. Each question should offer the earlier answer as its default, so that going
back shows what was chosen.

### Help pages

How commands and options are declared belongs to each implementation (the shell one has its own
`cli_cmd`; the Rust one builds on clap). What an agent gets for `help` is shared: the whole
reference in one page.

```
todo 1.0.0 - A tiny todo list
usage: todo <command> [args]
commands:
  add <text>  Add a task
    -p, --priority <n>  1 (high) to 3 (low) (default: 2)
  list  Show the tasks
    -a, --all  Include finished tasks
options:
  -v, --verbose  Say more
```

Help for one command is `todo add <text>  Add a task` followed by its options, indented by two
spaces. An unknown command gives `error: unknown command 'lst'; commands: add list` and status 2.

## Conformance

`conformance/cases.txt` is a list of cases. Each one names a mode, makes some calls, and says
exactly what must come out.

```
[agent] select by number
$ select Fruit apple banana cherry
< 2
|banana
!Fruit: banana
```

| Line | Meaning |
|---|---|
| `[mode] name` | starts a case |
| `% NAME=value` | an environment variable for the case |
| `$ function args` | a call, with shell quoting; several calls run in one process |
| `< text` | a line of standard input; `<` alone is an empty line |
| `\|text` | an expected line of standard output |
| `!text` | an expected line of standard error |
| `= N` | the expected exit status of the last call; 0 when absent |

In input and expected lines, `\e` is escape, `\t` tab, `\r` carriage return, `\s` a space and
`\b` backspace (the DEL character). An expected line that ends in `\c` has no newline after it.
`<T>` stands for any duration. Cases run in the `conformance` directory, and the `tree -p` and
`browse` cases read the files in `conformance/files`.

Every case runs with `COLUMNS=80`, `LANG=en_US.UTF-8`, no other variables from this file, and no
terminal. Cases with `CLI_INTERACTIVE=1` feed key presses on standard input.

To add an implementation, write a driver that maps each function name in the file to that
implementation's call, and run every case. `sh/test/conform.sh` and `rust/tests/conformance.rs`
are the two that exist.

`conformance/bless.py` rewrites the expected lines from what the shell implementation prints. To
add a case, write its `[mode] name`, `%`, `$` and `<` lines, run it, and read the diff: it records
what the reference does, right or wrong.

What the cases do not cover: animation frames, `die` and `require` (they exit), wizards and the
interactive prompt (each implementation tests its own), and real terminals.
`conformance/terminal.py` covers one thing there: it starts every key-driven prompt in a
pseudo-terminal, presses Ctrl-C, and checks that the terminal is left the way it was found.
