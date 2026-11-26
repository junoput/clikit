#!/usr/bin/env bash
# Try every feature of cli.sh. Each section shows the code it runs ("> ...") and then runs it.
#
#   examples/demo.sh                  a menu of sections (in a terminal)
#   examples/demo.sh all              every section, top to bottom
#   examples/demo.sh prompts          one section
#   examples/demo.sh mock             a pretend release that uses most features together
#   examples/demo.sh mock --fail      the same, with failing tests
#   examples/demo.sh compare lists    one section in rich, plain and agent mode
#   examples/demo.sh help             the list of sections
#
#   CLI_MODE=plain examples/demo.sh all     what a log file gets
#   CLI_AGENT=1    examples/demo.sh all     what an LLM gets
#
# DEMO_FAST=1 skips the waiting.
set -euo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
. "$HERE/../cli.sh"

COUNT=3
if [ -n "${DEMO_FAST:-}" ]; then COUNT=0; fi
IN_ALL=''
TODO_TMP=''

try() {  # show some code, then run it
  local line
  while IFS= read -r line; do
    printf '%s> %s%s\n' "$CLI_DIM" "$line" "$CLI_RESET"
  done <<<"$1"
  eval "$1"
}

work() {  # pretend to be busy for a while
  if [ -z "${DEMO_FAST:-}" ]; then sleep "$1"; fi
}

todo() {  # the example app next to this file, with a throwaway data file
  if [ -z "$TODO_TMP" ]; then TODO_TMP=$(mktemp "${TMPDIR:-/tmp}/cli-demo.XXXXXX"); fi
  TODO_FILE=$TODO_TMP "$HERE/todo.sh" "$@"
}

# ── sections ─────────────────────────────────────────────────────────────────

demo_messages() {
  cli_header "Messages"
  try 'cli_info "Something worth knowing"'
  try 'cli_ok "It worked"'
  try 'cli_warn "Look out"'
  try 'cli_error "It broke"'
  try 'cli_hint "A tip that only humans see"'
  try 'CLI_DEBUG=1 cli_debug "Shown only with CLI_DEBUG=1"'
  try 'cli_steps 3
cli_step "Fetch"
cli_step "Build"
cli_step "Ship"'
  try 'cli_exec uname -s'
  try 'cli_style "bold magenta" "Styled text"'
  try 'echo "${CLI_GREEN}green${CLI_RESET}, ${CLI_BOLD}bold${CLI_RESET}, ${CLI_UNDERLINE}underlined${CLI_RESET}"'
}

demo_layout() {
  try 'cli_header "A header"'
  try 'cli_box "A box with one line"'
  try 'cli_box -t "Note" "Plain text in a frame." "Agents get the text without the frame."'
  try 'cli_hr'
  try 'printf "indented\nlines\n" | cli_indent 6'
  try 'cli_nl'
}

demo_lists() {
  cli_header "Lists, key/values, tables"
  try 'cli_list "apples" "bananas" "cherries"'
  try 'cli_list -n "first" "second" "third"'
  try 'printf "from\nstandard\ninput\n" | cli_list'
  try 'cli_kv "host" "example.org" "port" "443" "protocol" "https"'
  try 'cli_table "Name  | Role   | Since" \
          "Ana   | dev    | 2019" \
          "Björn | ops    | 2021" \
          "Chidi | design | 2024"'
  try 'printf "user,shell\nroot,/bin/sh\nana,/bin/zsh\n" | cli_table -s ,'
  cli_hint "Columns that hold only numbers are aligned right:"
  try 'cli_table "Item | Qty | Price" "Apple | 3 | 1.20" "Melon | 12 | 10.00" "Fig | 100 | 0.50"'
  cli_hint "-b draws borders, -a sets the alignment per column (l, r or c):"
  try 'cli_table -b -a lcr "Name | Role | Since" "Ana | dev | 2019" "Björn | ops | 2021"'
  cli_hint "-H means there is no header: just aligned columns."
  try 'cli_table -H "config | ~/.config/app.toml" "cache | ~/.cache/app" "log | /var/log/app.log"'
  cli_hint "At a terminal, a table that is too wide has its widest column cut:"
  try 'cli_table "File | What it is" "README.md | A long description that will not fit on one line of most terminals, so the end of it is replaced by an ellipsis when a person is looking"'
  cli_hint "-k keeps a column whole, an identifier for one: the others are cut instead. Cells may be"
  cli_hint "coloured, and emoji and CJK text take the two columns a terminal gives them:"
  try 'cli_table -k Id "Device | State | Id" \
          "📱 Kitchen tablet | $(cli_style green online) | 00008030-000123456789ABCD" \
          "A phone with a rather long name, longer than fits here | $(cli_style red failed) | 00008150-001A2B3C4D5E6F78" \
          "日本語の名前 | $(cli_style dim never) | 00008101-000A1B2C3D4E5F60"'
  cli_hint "Each stream is judged on its own: in \"demo.sh lists | cat\" the tables are plain and these hints keep their look."
}

demo_time() {
  cli_header "Time"
  try 'cli_timer_start
cli_timer_start build
work 0.4
echo "build took $(cli_timer_elapsed build)"'
  try 'cli_time "Sleeping" work 0.3'
  try 'cli_countdown $COUNT "Continuing in"'
  try 'cli_duration 340; cli_duration 4250; cli_duration 187000; cli_duration 3725000'
  try 'echo "this section took $(cli_timer_elapsed), or $(cli_timer_ms) ms"'
}

demo_progress() {
  cli_header "Progress"
  try 'i=0
while [ $i -lt 40 ]; do
  i=$((i + 1)); work 0.04
  cli_progress $i 40 "Downloading"
done'
  try 'i=0
while [ $i -lt 12 ]; do
  i=$((i + 1)); work 0.04
  cli_progress $i 40 "Stopping early"
done
cli_progress_done'
  cli_hint "Several tasks under one total. One after the other they are the stages of a job:"
  try 'cli_tasks "Backup" "Scan | 1" "Photos | 6" "Messages | 2" "Verify | 1"
cli_task Scan; work 0.6; cli_task_done Scan
i=0; while [ $i -lt 30 ]; do i=$((i + 1)); work 0.04; cli_task Photos $i 30; done
i=0; while [ $i -lt 12 ]; do i=$((i + 1)); work 0.04; cli_task Messages $i 12; done
cli_task Verify; work 0.5; cli_task_done Verify
cli_tasks_done'
  cli_hint "Running at the same time they are bars stacked over the total. A task can fail:"
  try 'cli_tasks "Sync" iPhone iPad Watch
i=0
while [ $i -lt 40 ]; do
  i=$((i + 1)); work 0.04
  cli_task iPhone $i 40
  if [ $i -le 25 ]; then cli_task iPad $((i * 8)) 200; fi
  if [ $i -lt 12 ]; then cli_task Watch $i 30; elif [ $i -eq 12 ]; then cli_task_fail Watch "not reachable"; fi
done
cli_tasks_done || echo "exit status $?"'
  cli_hint "A task can show sizes, its speed and the time left, or only a percentage. A note says what it is"
  cli_hint "doing, a task can be skipped, and -t gives times that are known, for work that began earlier:"
  try 'cli_tasks -t 95 "Restore" Unlock "Photos | 6 | bytes rate eta" "Settings | 1 | percent"
cli_task_skip Unlock
i=0
while [ $i -lt 20 ]; do
  i=$((i + 1)); work 0.08
  cli_task -t 95 Photos $((1073741824 + i * 53687091)) 2147483648
  cli_task Settings $((i * 5)) 100
  case $i in 5) cli_task_note Settings "wallpaper";; 12) cli_task_note Settings "accounts";; esac
  if [ $i -eq 10 ]; then cli_warn "A message goes above the bars, not through them"; fi
done
cli_task_done Photos; cli_task_note Photos "1,204 files"
cli_tasks_done'
  try 'cli_spin "Compiling" work 1.5'
  try 'cli_spin "A step that fails" sh -c "echo line 12: something is wrong; exit 3" || echo "exit status $?"'
}

demo_prompts() {
  cli_header "Prompts"
  cli_hint "Escape goes back one question. Tab and Shift-Tab step through options and suggestions."
  cli_hint "With no terminal (or for an agent) these read a line of stdin, or take the default."
  try 'ask_name()     { name=$(cli_input "Your name" "${name:-stranger}"); }
ask_branch()   { branch=$(cli_input -c main -c develop -c release -c hotfix "Branch (try Tab)" "${branch:-main}"); }
ask_fruit()    { fruit=$(cli_select -d "${fruit:-banana}" "Favourite fruit" apple banana cherry "dragon fruit"); }
ask_city()     { city=$(cli_select -d "${city:-Lisbon}" "A long list scrolls" Amsterdam Berlin Copenhagen Dublin \
                   Edinburgh Florence Geneva Helsinki Istanbul Lisbon Madrid Oslo Prague Rome Stockholm Vienna); }
ask_toppings() { toppings=$(cli_multiselect -d nuts "Toppings" nuts honey cream chocolate); }
ask_save()     { saved=$(cli_confirm -p "Save this?" y); }
cli_wizard ask_name ask_branch ask_fruit ask_city ask_toppings ask_save || echo "left the wizard (status $?)"'
  try 'secret=$(cli_password "A password (typing is hidden)") || secret=""'
  try 'cli_pause'
  try 'cli_kv name "${name:-}" branch "${branch:-}" fruit "${fruit:-}" city "${city:-}" \
       toppings "${toppings//$_CLI_NL/, }" saved "${saved:-}" "password length" "${#secret}"'
}

demo_commands() {
  cli_header "Commands and help pages"
  cli_hint "examples/todo.sh is a small app made with cli_app, cli_cmd, cli_opt and cli_run."
  try 'todo'
  try 'todo help add'
  try 'todo add "Buy milk" -p 1'
  try 'todo add Call mum'
  try 'todo list'
  try 'todo done 1'
  try 'todo list --all'
  try 'todo ad "typo" || echo "exit status $?"'
  try 'todo add --nope || echo "exit status $?"'
  try 'todo --version'
  try 'todo --agent help'
  rm -f "$TODO_TMP"; TODO_TMP=''
}

demo_interactive() {
  cli_header "Interactive mode"
  try 'printf "add \"From a pipe\"\nlist\n" | todo -i'
  if [ -z "$IN_ALL" ] && [ "$(cli_mode)" = rich ] && [ -t 0 ]; then
    cli_hint "Now live. Tab and Shift-Tab step through commands and options, up-arrow recalls a line,"
    cli_hint "Ctrl-C drops the line, and on an empty line leaves the prompt."
    try 'todo -i'
  else
    cli_info "Run 'examples/demo.sh interactive' in a terminal for the live prompt."
  fi
  rm -f "$TODO_TMP"; TODO_TMP=''
}

fake_tests() {
  work 0.9
  printf 'test parse_empty ... ok\ntest parse_dates ... FAILED\n\n  expected: 2026-01-31\n    actual: 2026-02-00\n\n127 passed, 1 failed\n'
  [ -z "$opt_fail" ]
}

demo_mock() {  # a release pipeline in which nothing real happens
  local target=$opt_env i=0 total=30 t_build t_test t_upload
  cli_header "A pretend release"
  cli_timer_start release
  cli_require git tar
  cli_kv "project" "demo-app" "version" "2.4.0" "branch" "main"
  cli_nl
  cli_steps 5

  cli_step "Check the workspace"
  cli_spin "Looking for uncommitted changes" work 0.6

  cli_step "Build"
  cli_timer_start build
  cli_spin "Compiling 42 files" work 1.4
  t_build=$(cli_timer_elapsed build)

  cli_step "Test"
  cli_timer_start test
  if ! cli_spin "Running 128 tests" fake_tests; then
    cli_error "Release stopped: the tests failed"
    cli_hint "Run it again without --fail to see the rest."
    return 1
  fi
  t_test=$(cli_timer_elapsed test)

  cli_step "Upload"
  if [ -z "$target" ]; then
    target=$(cli_select -d staging "Deploy to" staging production) || { cli_warn "Cancelled"; return 0; }
  fi
  cli_timer_start upload
  while [ "$i" -lt "$total" ]; do
    i=$((i + 1)); work 0.05
    cli_progress "$i" "$total" "Uploading to $target"
  done
  t_upload=$(cli_timer_elapsed upload)

  cli_step "Publish"
  if [ "$target" = production ] && ! cli_confirm "Really publish to production?" n; then
    cli_warn "Stopped before publishing"
    return 0
  fi
  cli_countdown "$COUNT" "Publishing in"
  cli_spin "Switching traffic" work 0.5

  cli_header "Result"
  cli_table "Stage  | Result       | Time" \
            "Build  | 42 files     | $t_build" \
            "Test   | 128 passed   | $t_test" \
            "Upload | $total files | $t_upload"
  cli_nl
  cli_box -t "Released" "demo-app 2.4.0 is live on $target" "https://$target.example.org"
  cli_ok "Finished in $(cli_timer_elapsed release)"
}

demo_compare() {  # one section, three times
  local section=${1:-messages} mode
  if ! typeset -f "demo_$section" >/dev/null 2>&1; then cli_die "no such section: $section" 2; fi
  for mode in rich plain agent; do
    cli_header "$section, $mode mode"
    ( cli_mode "$mode"; "demo_$section" ) || true
  done
}

demo_all() {
  local name
  IN_ALL=1
  for name in "${NAMES[@]}"; do "demo_$name"; done
}

# ── the app ──────────────────────────────────────────────────────────────────

backups() {  # the snapshots of three devices: WHEN | TRACK | LABEL | FROM
  cat <<'ROWS'
2024-02-01 | iPhone 12 | 38.7 GB
2024-05-03 | iPhone 12 | 39.1 GB
2024-07-11 | iPad      | 11.9 GB
2025-01-15 | iPhone 12 | 40.2 GB
2025-02-02 | iPad      | 12.3 GB
2025-06-01 | iPhone 15 | continues iPhone 12 | iPhone 12
2025-09-14 | iPhone 15 | 43.0 GB
2025-11-20 | iPad      | 12.8 GB
2026-04-02 | iPhone 15 | 44.1 GB
2026-09-14 | iPhone 15 | combined with iPad  | iPad
2026-11-20 | iPhone 15 | 44.9 GB
ROWS
}

demo_timeline() {
  cli_header "Timelines"
  cli_hint "Rows are \"WHEN | TRACK | LABEL | FROM\". These are the snapshots of three devices:"
  try 'backups | cli_indent 4'
  cli_hint "One line per track, as wide as the terminal. FROM draws the link between two tracks:"
  try 'backups | cli_timeline'
  cli_hint "-v: one row per event. As long as it needs to be, and it has room for the labels:"
  try 'backups | cli_timeline -v'
  cli_hint "-r puts the newest on top, like a commit history:"
  try 'cli_timeline -v -r \
  "2026-11-02 09:10 | main    | Initial commit" \
  "2026-11-03 14:25 | main    | Add the parser" \
  "2026-11-04 10:00 | feature | Start the new output | main" \
  "2026-11-05 16:40 | main    | Fix an off-by-one" \
  "2026-11-06 11:15 | feature | Tables" \
  "2026-11-09 13:30 | main    | Merge feature | feature" \
  "2026-11-10 08:05 | main    | Release 1.1"'
  cli_hint "A track can be anything. The axis picks years, months, days or hours:"
  try 'cli_timeline \
  "2026-06-27 | Rome     |" "2026-06-28 | Rome     |" "2026-06-28 | Rome     |" "2026-07-02 | Rome     |" \
  "2026-07-18 | Wedding  |" "2026-07-19 | Wedding  |" \
  "2026-08-03 | Hiking   |" "2026-08-09 | Hiking   |" "2026-08-21 | Hiking   |"'
  cli_hint "WHEN can be any text: then rows keep the order they were given in."
  try 'cli_timeline "v1.0 | stable | " "v1.1 | stable | " "v2.0-beta | next | | stable" "v1.2 | stable | " "v2.0 | stable | | next"'
  cli_hint "Without track names it is one line:"
  try 'cli_timeline -v "09:15 | | Breakfast" "12:40 | | Lunch" "19:05 | | Dinner"'
}

FILES=(  # what a backup holds: PATH | NOTE
  "Photos/2024/rome.jpg    | 2.1 MB"
  "Photos/2024/wedding.jpg | 3.4 MB"
  "Photos/2025/hiking.jpg  | 1.8 MB"
  "Notes/todo.txt          | 4 KB"
  "Notes/ideas.md          | 12 KB"
  "Contacts.vcf            | 310 KB"
)

demo_tree() {
  local picked='' folder=''
  cli_header "Trees"
  cli_hint "Rows are \"PATH | NOTE\". Anything that nests: the files in a backup, settings, chapters."
  try 'cli_tree "${FILES[@]}"'
  cli_hint "-d: so many levels. A directory with more inside ends in an ellipsis:"
  try 'cli_tree -d 1 "${FILES[@]}"'
  cli_hint "-p: the files under a directory (-a: hidden ones too)."
  try '(cd "$HERE/.." && cli_tree -p .)'
  cli_hint "cli_browse walks through a tree and prints the path that was chosen."
  if _cli_tty; then
    cli_hint "Right opens, left closes, enter chooses. Left on the top directory shows the one above it."
    try 'picked=$(cd "$HERE/.." && cli_browse "Pick a file") || picked="(nothing)"'
    cli_hint "-l walks through a list of paths, -d chooses a directory:"
    try 'folder=$(cli_browse -l -d "Restore which folder" "${FILES[@]}") || folder="(nothing)"'
  else
    cli_hint "Without a terminal (or for an agent) it reads the path from stdin:"
    try 'picked=$(cd "$HERE/.." && echo cli.sh | cli_browse "Pick a file")'
    try 'folder=$(echo Photos/2024 | cli_browse -l -d "Restore which folder" "${FILES[@]}")'
  fi
  try 'cli_kv file "$picked" folder "$folder"'
}

NAMES=()
MENU=()
section() {  # NAME DESCRIPTION
  cli_cmd "$1" "$2" "demo_$1"
  NAMES+=("$1")
  MENU+=("$(printf '%-12s %s' "$1" "$2")")
}

cli_app demo.sh "Try every feature of cli.sh" "$CLI_VERSION"
section messages "Status lines, steps, styled text"
section layout "Headers, boxes, rules"
section lists "Lists, key/values, tables"
section timeline "Events on tracks: along a time axis, or one per row"
section tree "Trees of paths or files, and walking through one"
section time "Stopwatches, countdown, durations"
section progress "Progress bars, tasks under one total, spinner"
section prompts "Input, selectors, confirm, password"
section commands "An app with commands, options and help pages"
section interactive "The interactive prompt of that app"
section mock "A pretend release that uses most features together"
cli_opt "-e, --env <name>" "Deploy target (staging, production); skips the question"
cli_opt "--fail" "Make the tests fail, to see the error path"
cli_cmd compare "One section in rich, plain and agent mode" demo_compare
cli_args "<section>"
cli_cmd all "Every section, top to bottom" demo_all

menu() {
  local choice
  while :; do
    choice=$(CLI_PAGE=20 cli_select "What do you want to see?" "${MENU[@]}" \
      "compare      One section in rich, plain and agent mode" \
      "all          Every section, top to bottom" \
      "quit") || break
    choice=${choice%% *}
    case $choice in
      quit) break;;
      compare) choice=$(cli_select "Which section?" "${NAMES[@]}") || continue
               demo_compare "$choice";;
      *) "demo_$choice" || true;;
    esac
    cli_nl
    cli_pause "Press any key for the menu"
  done
}

if [ $# -eq 0 ] && [ "$(cli_mode)" = rich ] && [ -t 0 ]; then
  menu
else
  cli_run "$@"
fi
