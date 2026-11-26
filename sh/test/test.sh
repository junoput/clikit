#!/usr/bin/env bash
# Tests for cli.sh. Run under one shell:   bash test/test.sh   |   zsh test/test.sh
# or under every shell on this machine:    test/all.sh
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
LIB="$HERE/../cli.sh"
if [ -n "${ZSH_VERSION:-}" ]; then SH=zsh; else SH=${BASH:-bash}; fi

unset CLI_MODE NO_COLOR FORCE_COLOR CLI_QUIET CLI_DEBUG CLI_YES CLI_INTERACTIVE
export CLI_AGENT=0 COLUMNS=80
# shellcheck source=../cli.sh
. "$LIB"

PASS=0 FAIL=0
NL=$'\n' TAB=$'\t' ESC=$'\033'

check() {  # NAME EXPECTED ACTUAL
  if [ "$2" = "$3" ]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$1" "$(printf '%s' "$2" | od -c | head -5)" "$(printf '%s' "$3" | od -c | head -5)"
  fi
}
has() {  # NAME NEEDLE HAYSTACK
  case $3 in
    *"$2"*) PASS=$((PASS + 1));;
    *) FAIL=$((FAIL + 1)); printf 'FAIL %s\n  missing: %s\n  in: %s\n' "$1" "$2" "$3";;
  esac
}
hasnt() {  # NAME NEEDLE HAYSTACK
  case $3 in
    *"$2"*) FAIL=$((FAIL + 1)); printf 'FAIL %s\n  unwanted: %s\n  in: %s\n' "$1" "$2" "$3";;
    *) PASS=$((PASS + 1));;
  esac
}
status() {  # COMMAND...   print its exit status, nothing else
  local s=0
  ( "$@" ) >/dev/null 2>&1 || s=$?
  printf '%s\n' "$s"
}
strip() { sed "s/${ESC}\[[0-9;]*[A-Za-z]//g"; }  # drop escape sequences

# ── modes ────────────────────────────────────────────────────────────────────
check "auto mode without a tty is plain" plain "$(cli_mode)"
check "CLI_AGENT=1 selects agent" agent "$(CLI_AGENT=1 "$SH" -c '. "$1"; cli_mode' _ "$LIB")"
check "a harness variable selects agent" agent "$(unset CLI_AGENT; CLAUDECODE=1 "$SH" -c '. "$1"; cli_mode' _ "$LIB")"
check "CLI_AGENT=0 forbids detection" plain "$(CLAUDECODE=1 "$SH" -c '. "$1"; cli_mode' _ "$LIB")"
check "CLI_MODE wins" rich "$(CLI_MODE=rich CLI_AGENT=1 "$SH" -c '. "$1"; cli_mode' _ "$LIB")"
check "FORCE_COLOR gives rich" rich "$(FORCE_COLOR=1 "$SH" -c '. "$1"; cli_mode' _ "$LIB")"
check "NO_COLOR keeps rich but drops colour" "rich:hi" "$(CLI_MODE=rich NO_COLOR=1 "$SH" -c '. "$1"; printf "%s:" "$(cli_mode)"; cli_style red hi' _ "$LIB")"

# ── messages ─────────────────────────────────────────────────────────────────
cli_mode plain
check "plain info" "[INFO] hello world" "$(cli_info hello world 2>&1)"
check "plain ok" "[OK] done" "$(cli_ok done 2>&1)"
check "plain warn" "[WARN] careful" "$(cli_warn careful 2>&1)"
check "plain error" "[ERROR] broke" "$(cli_error broke 2>&1)"
check "messages go to stderr" "" "$(cli_info hidden 2>/dev/null)"
check "debug is off by default" "" "$(cli_debug x 2>&1)"
check "debug shows with CLI_DEBUG" "[DEBUG] x" "$(CLI_DEBUG=1 cli_debug x 2>&1)"
check "quiet drops info" "" "$(CLI_QUIET=1 cli_info x 2>&1)"
check "quiet keeps errors" "[ERROR] x" "$(CLI_QUIET=1 cli_error x 2>&1)"
check "die prints the error" "[ERROR] bye" "$( (cli_die bye 7) 2>&1 || true)"
check "die exits with the code" 7 "$(status cli_die bye 7)"
check "plain hint" "tip" "$(cli_hint tip 2>&1)"
check "plain steps" "==> [1/2] a${NL}==> [2/2] b" "$(cli_steps 2; cli_step a 2>&1; cli_step b 2>&1)"
check "plain style has no codes" "x y" "$(cli_style 'bold red' x y)"

cli_mode agent
check "agent info is bare" "hello" "$(cli_info hello 2>&1)"
check "agent ok" "ok: done" "$(cli_ok done 2>&1)"
check "agent warn" "warn: careful" "$(cli_warn careful 2>&1)"
check "agent error" "error: broke" "$(cli_error broke 2>&1)"
check "agent hides hints" "" "$(cli_hint tip 2>&1)"
check "agent steps" "[1/2] a" "$(cli_steps 2; cli_step a 2>&1)"
check "agent step without a total" "a" "$(cli_steps 0; cli_step a 2>&1)"

cli_mode rich
check "rich info" "● hello" "$(cli_info hello 2>&1 | strip)"
has "rich uses colour" "${ESC}[32m" "$(cli_ok done 2>&1)"
check "style" "${ESC}[1m${ESC}[31mhi${ESC}[0m" "$(cli_style 'bold red' hi)"
check "style ignores unknown names" "${ESC}[4mhi${ESC}[0m" "$(cli_style 'nope underline' hi)"

# ── layout, lists, tables ────────────────────────────────────────────────────
cli_mode agent
check "agent header" "# Title here" "$(cli_header Title here)"
check "agent hr is nothing" "" "$(cli_hr)"
check "agent nl is nothing" "" "$(cli_nl)"
check "agent box" "note:${NL}a${NL}b" "$(cli_box -t note a b)"
check "agent list" "- a${NL}- b c" "$(cli_list a 'b c')"
check "agent numbered list" "1. a${NL}2. b" "$(cli_list -n a b)"
check "agent kv" "name: ana${NL}role: dev" "$(cli_kv name ana role dev)"
check "agent table is tab separated" "Name${TAB}Role${NL}Ana${TAB}dev" "$(cli_table 'Name | Role' 'Ana | dev')"
check "agent timeline is the events in order" "1${TAB}a${TAB}x${NL}2${TAB}b${TAB}y${TAB}from: a" "$(cli_timeline '2|b|y|a' '1|a|x')"

cli_mode plain
check "plain header" "${NL}== Title ==" "$(cli_header Title)"
check "plain list" "  - a${NL}  - b" "$(cli_list a b)"
check "list from stdin" "  - a${NL}  - b" "$(printf 'a\nb\n' | cli_list)"
check "numbered list aligns" "   9. i${NL}  10. j" "$(cli_list -n a b c d e f g h i j | tail -2)"
check "plain kv aligns" "  name:    ana${NL}  country: ch" "$(cli_kv name ana country ch)"
check "plain table" "  Name   Role${NL}  -----  ----${NL}  Ana    dev${NL}  Björn  ops" "$(cli_table 'Name|Role' 'Ana|dev' 'Björn|ops')"
check "table with another separator" "  a   b${NL}  --  -${NL}  cc  d" "$(cli_table -s , 'a,b' 'cc,d')"
check "table from stdin" "  a  b${NL}  -  -${NL}  c  d" "$(printf 'a|b\nc|d\n' | cli_table)"
check "table aligns numbers right" "  Item   Qty${NL}  -----  ---${NL}  Apple    3${NL}  Melon   12" "$(cli_table 'Item|Qty' 'Apple|3' 'Melon|12')"
check "table with borders" "+-------+-----+${NL}| Item  | Qty |${NL}+-------+-----+${NL}| Apple |   3 |${NL}+-------+-----+" "$(cli_table -b 'Item|Qty' 'Apple|3')"
check "table without a header" "  a    1${NL}  bbb  2" "$(cli_table -H 'a|1' 'bbb|2')"
check "table with explicit alignment" "    a  b${NL}  ---  -${NL}  ccc  d" "$(cli_table -a rl 'a|b' 'ccc|d')"
check "table options combine" "+---+---+${NL}| a | 1 |${NL}+---+---+" "$(printf 'a,1\n' | cli_table -b -H -s ,)"
check "ragged table" "  a  b  c${NL}  -  -  -${NL}  d" "$(cli_table 'a|b|c' 'd')"
check "timeline, one row per event" "  *    1  a  x${NL}  +-*  2  b  y" "$(cli_timeline -v '2|b|y|a' '1|a|x')"
check "timeline from stdin" "  *  1  a  x${NL}  *  2  a  y" "$(printf '1|a|x\n2|a|y\n' | cli_timeline -v)"
check "timeline along an axis" "     1          9${NL}  a  *-----+${NL}  b        *----*" "$(COLUMNS=17 cli_timeline '9|b|' '1|a|' '5|b||a')"
check "timeline with another separator, newest first" "  *  2  a${NL}  *  1  a" "$(cli_timeline -v -r -s , '1,a' '2,a')"
check "timeline leaves no variables behind" "" "$(cli_timeline -v '1|a|x' '2|b|y|a' >/dev/null; cli_timeline '1|a|x' '2|b|y|a' >/dev/null; set | grep '^_cli_t[a-z]*[0-9]' || true)"
check "timeline run as a command" "  *  1  a  x" "$(CLI_MODE=plain "$SH" "$LIB" timeline -v '1|a|x')"
check "tree from stdin" "a/${NL}|-- b${NL}\`-- c  n${NL}d" "$(printf 'a/b\na/c | n\nd\n' | cli_tree)"
check "tree run as a command" "a/${NL}\`-- b" "$(CLI_MODE=plain "$SH" "$LIB" tree a/b)"
check "agent tree" "a/${NL} b${TAB}n" "$(CLI_MODE=agent "$SH" "$LIB" tree 'a/b|n')"
TREE=$(mktemp -d "${TMPDIR:-/tmp}/cli-tree.XXXXXX")
mkdir -p "$TREE/top/b/inner" "$TREE/top/a" "$TREE/elsewhere/deep"
: >"$TREE/top/b/inner/x"; : >"$TREE/top/Z"; : >"$TREE/top/.dot"; : >"$TREE/elsewhere/deep/y"
: >"$TREE/top/a/$(printf 'e\033[31mx')"
ln -s ../elsewhere "$TREE/top/link"; ln -s nowhere "$TREE/top/broken"
check "tree of files: directories first, links not followed, escape codes defused" \
  "top/${NL}|-- a/${NL}|   \`-- e?[31mx${NL}|-- b/${NL}|   \`-- inner/${NL}|       \`-- x${NL}|-- link/ ...${NL}|-- Z${NL}\`-- broken" \
  "$(cd "$TREE" && cli_tree -p top)"
check "tree of files: hidden ones" "top/b/${NL}\`-- inner/ ...${NL}10" "$(cd "$TREE" && cli_tree -d 1 -p top/b && cli_tree -a -p top | wc -l | tr -d ' ')"
check "tree leaves no variables behind" "" "$(cd "$TREE" && cli_tree -p top >/dev/null; cli_tree a/b c >/dev/null; set | grep '^_cli_[fv][a-z]*[0-9]' || true)"
check "browse the current directory: no ./ in the answer" "Z" "$(cd "$TREE/top" && printf 'jjjj\n' | CLI_INTERACTIVE=1 cli_browse Pick 2>/dev/null)"
check "browse goes above the current directory" "../a" "$(cd "$TREE/top/b" && printf 'hk\n' | CLI_INTERACTIVE=1 cli_browse -d Pick 2>/dev/null)"
check "browse goes above an absolute directory" "$TREE/top/a" "$(printf 'hk\n' | CLI_INTERACTIVE=1 cli_browse -d Pick "$TREE/top/b" 2>/dev/null)"
check "browse walks through a link to a directory" "top/link/deep/y" "$(cd "$TREE" && printf 'jjjljlj\n' | CLI_INTERACTIVE=1 cli_browse Pick top 2>/dev/null)"
check "browse shows hidden files on request" "top/.dot" "$(cd "$TREE" && printf 'jjjj\n' | CLI_INTERACTIVE=1 cli_browse -a Pick top 2>/dev/null)"
check "browse leaves no variables behind" "" "$(cd "$TREE" && printf 'jl\033' | CLI_INTERACTIVE=1 cli_browse Pick top >/dev/null 2>&1 || true; set | grep '^_cli_[fv][a-z]*[0-9]' || true)"
check "browse without keys takes a path" "top/Z" "$(cd "$TREE" && printf 'Z\n' | cli_browse Pick top 2>/dev/null)"
check "browse run as a command" "a/b" "$(printf 'l\033[B\n' | CLI_MODE=rich CLI_INTERACTIVE=1 "$SH" "$LIB" browse -l Pick a/b c 2>/dev/null)"
rm -rf "$TREE"
check "plain box" "+- hi ---+${NL}| one    |${NL}| three3 |${NL}+--------+" "$(cli_box -t hi one three3)"
check "box without title" "+----+${NL}| ab |${NL}+----+" "$(cli_box ab)"
check "indent" "    a${NL}    b" "$(printf 'a\nb\n' | cli_indent 4)"
check "hr is 80 wide" 80 "$(cli_hr | awk '{ print length($0) }')"

cli_mode rich
check "rich box" "╭─ hi ─╮${NL}│ one  │${NL}╰──────╯" "$(cli_box -t hi one | strip)"
check "rich list" "  • a" "$(cli_list a | strip)"
check "rich header" "${NL}Two words${NL}─────────" "$(cli_header Two words | strip)"
check "rich table" "  Name   Role${NL}  ─────  ────${NL}  Björn  ops" "$(cli_table 'Name|Role' 'Björn|ops' | strip)"
check "rich timeline" "  ●    1  a  x${NL}  ╰─●  2  b  y" "$(cli_timeline -v '2|b|y|a' '1|a|x' | strip)"
check "rich tree" "a/${NL}├── b  n${NL}╰── c/ …" "$(cli_tree -d 2 'a/b|n' a/c/d | strip)"
check "rich timeline colours each track" "  ${ESC}[36ma  ●${ESC}[0m${NL}  ${ESC}[35mb  ●${ESC}[0m" "$(COLUMNS=20 cli_timeline '1|a|' '1|b|' | tail -2)"

# ── time ─────────────────────────────────────────────────────────────────────
check "duration ms" 340ms "$(cli_duration 340)"
check "duration s" 4.2s "$(cli_duration 4250)"
check "duration min" "3m 07s" "$(cli_duration 187000)"
check "duration h" "1h 02m" "$(cli_duration 3725000)"
case $(cli_now_ms) in *[!0-9]*|'') check "now_ms is a number" number "$(cli_now_ms)";; *) check "now_ms is a number" ok ok;; esac
cli_timer_start
cli_timer_start "build one"
sleep 1
ms=$(cli_timer_ms "build one")
if [ "$ms" -ge 900 ] && [ "$ms" -lt 3000 ]; then check "timer measures a second" ok ok; else check "timer measures a second" "900..3000" "$ms"; fi
case $(cli_timer_elapsed) in 1s|1.[0-9]s|2.[0-9]s) check "timer elapsed is formatted" ok ok;; *) check "timer elapsed is formatted" "about 1s" "$(cli_timer_elapsed)";; esac
check "unknown timer has no elapsed time" 1 "$(status cli_timer_elapsed nope)"
check "unknown timer fails" "1" "$(cli_timer_ms nope >/dev/null 2>&1 || echo $?)"
cli_mode agent
has "time reports a duration" "nap: " "$(cli_time nap sleep 0 2>&1)"
check "time keeps the exit code" 3 "$(cli_time x sh -c 'exit 3' 2>/dev/null || echo $?)"
check "countdown is silent without a tty" "" "$(cli_countdown 0 2>&1)"

# ── progress and spinner ─────────────────────────────────────────────────────
cli_mode agent
out=$(for i in 1 2 3 4; do cli_progress "$i" 4 copy; done 2>&1)
has "agent progress is one line" "copy: 4/4 " "$out"
check "agent progress line count" 1 "$(printf '%s\n' "$out" | wc -l | tr -d ' ')"
has "agent progress ended early" "copy: 2/4 " "$(cli_progress 1 4 copy 2>&1; cli_progress 2 4 copy 2>&1; cli_progress_done 2>&1)"
cli_mode plain
check "plain progress prints quarters" "copy: 0% (0/8)${NL}copy: 25% (2/8)${NL}copy: 50% (4/8)${NL}copy: 75% (6/8)${NL}copy: 100% (8/8)" \
  "$(for i in 0 1 2 3 4 5 6 7 8; do cli_progress "$i" 8 copy; done 2>&1)"
cli_mode rich
has "rich tasks draw each bar and the total" "⠋ a    ██████░░░░░░░░░░░░░░░░░░  25%  1/4${NL}· b${NL}  Job  ███░░░░░░░░░░░░░░░░░░░░░  12%  0/2 " "$(cli_tasks Job a b 2>&1; cli_task a 1 4 2>&1 | strip)"
check "size" "1.5 KB 27.9 GB" "$(cli_size 1536) $(cli_size 30000000000)"
has "rich tasks show bytes, and a note" "  27.9 GB/68.9 GB  checking" "$(cli_tasks Copy 'Photos|1|bytes' 2>&1; cli_task Photos 30000000000 74000000000 2>/dev/null; cli_task_note Photos checking 2>&1 | strip)"
check "tasks: a failure is the exit status" 1 "$(status eval 'cli_tasks Job a; cli_task_fail a why; cli_tasks_done')"
check "tasks leave no variables behind" "" "$(cli_tasks Job a b 2>/dev/null; cli_task a 1 2 2>/dev/null; cli_task_done b 2>/dev/null; cli_tasks_done 2>/dev/null; set | grep '^_cli_k[a-z0-9]*[0-9]=' || true)"
has "rich progress draws a bar" "████████████░░░░░░░░░░░░  50% 2/4" "$(cli_progress 2 4 copy 2>&1 | strip)"

cli_mode agent
has "agent spin ok" "ok: build (" "$(cli_spin build true 2>&1)"
check "spin hides output on success" "" "$(cli_spin build echo noisy 2>/dev/null)"
out=$(cli_spin build sh -c 'echo boom; exit 4' 2>&1 || echo "rc=$?")
has "spin failure message" "error: build failed (exit 4" "$out"
has "spin failure shows output" "boom" "$out"
has "spin failure keeps the exit code" "rc=4" "$out"
out=$(cli_spin big sh -c 'seq 1 100; exit 1' 2>&1 || true)
has "agent spin keeps the tail" "(last 40 of 100 lines)" "$out"
hasnt "agent spin drops the head" "${NL}60${NL}" "$out"
spin_fn() { SPIN_SET=yes; }
cli_spin "a function" spin_fn 2>/dev/null
check "spin runs functions in this shell" yes "${SPIN_SET:-}"
cli_mode plain
check "plain spin" "[....] build${NL}[OK] build" "$(cli_spin build true 2>&1 | sed 's/ (.*//')"
cli_mode rich
has "rich spin ends with a tick" "✓ build (" "$(cli_spin build sleep 0.3 2>&1 | strip | tr '\r' '\n' | tail -1)"
check "exec shows and runs" "\$ echo hi${NL}hi" "$(cli_exec echo hi 2>&1 | strip)"

# ── prompts without a terminal ───────────────────────────────────────────────
cli_mode plain
check "select by number" banana "$(printf '2\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select by name" cherry "$(printf 'cherry\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select prefers a name over a number" 2 "$(printf '2\n' | cli_select N 3 2 1 2>/dev/null)"
check "select default on empty line" banana "$(printf '\n' | cli_select -d banana Fruit apple banana 2>/dev/null)"
check "select default on eof" banana "$(cli_select -d banana Fruit apple banana </dev/null 2>/dev/null)"
check "select without an answer fails" "rc=1" "$(cli_select Fruit apple banana </dev/null 2>/dev/null || echo "rc=$?")"
check "select rejects a bad answer" "rc=1" "$(printf 'kiwi\n' | cli_select Fruit apple banana 2>/dev/null || echo "rc=$?")"
check "plain select prompt" "Fruit${NL}  1) apple${NL}  2) banana${NL}choice [banana]: 1" "$(printf '1\n' | cli_select -d banana Fruit apple banana 2>&1 >/dev/null)"
check "option with spaces" "b c" "$(printf '2\n' | cli_select Pick a 'b c' 2>/dev/null)"
check "multiselect by numbers and names" "apple${NL}cherry" "$(printf '1, cherry\n' | cli_multiselect Fruit apple banana cherry 2>/dev/null)"
check "multiselect defaults" "a${NL}c" "$(cli_multiselect -d a -d c Pick a b c </dev/null 2>/dev/null)"
check "multiselect empty line is none" "rc=0" "rc=$(printf '\n' | status cli_multiselect Pick a b)"
check "multiselect without input fails" "rc=1" "$(cli_multiselect Pick a b </dev/null 2>/dev/null || echo "rc=$?")"
check "select without options" "rc=2" "$(cli_select Pick </dev/null 2>/dev/null || echo "rc=$?")"

check "confirm yes" 0 "$(printf 'y\n' | status cli_confirm Go)"
check "confirm no" 1 "$(printf 'no\n' | status cli_confirm Go)"
check "confirm default yes" 0 "$(status cli_confirm Go y </dev/null)"
check "confirm default no" 1 "$(status cli_confirm Go n </dev/null)"
check "confirm unanswered" 2 "$(status cli_confirm Go </dev/null)"
check "CLI_YES answers yes" 0 "$(CLI_YES=1 cli_confirm Go n </dev/null 2>/dev/null; echo $?)"
check "plain confirm prompt" "Go [Y/n] y" "$(cli_confirm Go y </dev/null 2>&1)"
check "input" "ana" "$(printf 'ana\n' | cli_input Name 2>/dev/null)"
check "input default" "bo" "$(cli_input Name bo </dev/null 2>/dev/null)"
check "input unanswered fails" "rc=1" "$(cli_input Name </dev/null 2>/dev/null || echo "rc=$?")"
check "plain input prompt" "Name [bo]: bo" "$(cli_input Name bo </dev/null 2>&1 >/dev/null)"
check "password" "s3cret" "$(printf 's3cret\n' | cli_password Token 2>/dev/null)"
check "pause does nothing" "" "$(cli_pause 2>&1)"

cli_mode agent
check "agent select echoes the answer" "Fruit: banana" "$(printf 'banana\n' | cli_select Fruit apple banana 2>&1 >/dev/null)"
check "agent select says how to answer" "error: input required: Fruit (options: apple | banana). Pipe the answer on stdin." \
  "$(cli_select Fruit apple banana </dev/null 2>&1 || true)"
check "agent confirm" "Go y" "$(printf 'y\n' | cli_confirm Go 2>&1)"
has "agent confirm unanswered" "error: input required: Go [y/n]" "$(cli_confirm Go </dev/null 2>&1 || true)"
check "agent input" "Name: bo" "$(cli_input Name bo </dev/null 2>&1 >/dev/null)"
t0=$(cli_now_ms)
out=$( (sleep 3 | cli_select -d a Pick a b) 2>/dev/null & wait)
check "agent mode does not wait on a silent pipe" a "$out"

# ── key-driven prompts (keys fed on stdin) ───────────────────────────────────
cli_mode rich
export CLI_INTERACTIVE=1
check "select: enter takes the first" apple "$(printf '\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select: arrow down" banana "$(printf '\033[B\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select: j j" cherry "$(printf 'jj\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select: up wraps around" cherry "$(printf '\033[A\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select: starts on the default" cherry "$(printf '\033[B\n' | cli_select -d banana Fruit apple banana cherry 2>/dev/null)"
check "select: esc cancels with status 3" "rc=3" "$(printf '\033' | cli_select Fruit apple banana 2>/dev/null || echo "rc=$?")"
check "select: tab and shift-tab move" banana "$(printf '\t\t\033[Z\n' | cli_select Fruit apple banana cherry 2>/dev/null)"
check "select: page up is not escape" apple "$(printf '\033[5~\n' | cli_select Fruit apple banana 2>/dev/null)"
check "select: eof cancels" "rc=1" "$(cli_select Fruit apple banana </dev/null 2>/dev/null || echo "rc=$?")"
check "select: scrolls a long list" 14 "$(printf 'jjjjjjjjjjjjj\n' | cli_select N 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 2>/dev/null)"
has "select: summary line" "✓ Fruit › banana" "$(printf 'j\n' | cli_select Fruit apple banana 2>&1 >/dev/null | strip)"
has "select: shows the page position" "6-15 of 15" "$(printf 'jjjjjjjjjjjjjj\n' | cli_select N 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 2>&1 >/dev/null | strip)"
check "multiselect: space toggles" "apple${NL}cherry" "$(printf ' jj \n' | cli_multiselect Fruit apple banana cherry 2>/dev/null)"
check "multiselect: toggle off" "banana" "$(printf ' j  \033[A \n' | cli_multiselect -d banana Fruit apple banana 2>/dev/null)"
check "multiselect: a selects all" "a${NL}b${NL}c" "$(printf 'a\n' | cli_multiselect Pick a b c 2>/dev/null)"
check "multiselect: nothing chosen" "" "$(printf '\n' | cli_multiselect Pick a b c 2>/dev/null)"
check "confirm: key y" 0 "$(printf 'y' | status cli_confirm Go)"
check "confirm: key n" 1 "$(printf 'n' | status cli_confirm Go)"
check "confirm: enter takes the default" 0 "$(printf '\n' | status cli_confirm Go y)"
check "confirm: enter without default waits" 1 "$(printf '\n\nn' | status cli_confirm Go)"
check "input: typed" "ana" "$(printf 'ana\n' | cli_input Name 2>/dev/null)"
check "input: default" "bo" "$(printf '\n' | cli_input Name bo 2>/dev/null)"
check "input: esc cancels with status 3" "rc=3" "$(printf 'ab\033\n' | cli_input Name 2>/dev/null || echo "rc=$?")"
check "input: tab cycles through the candidates" dev "$(printf '\t\t\n' | cli_input -c main -c dev -c fix Branch 2>/dev/null)"
check "input: shift-tab goes backwards" fix "$(printf '\033[Z\n' | cli_input -c main -c dev -c fix Branch 2>/dev/null)"
check "input: ctrl-u, ctrl-w, ctrl-a, ctrl-e, ctrl-k" "<one two >" "$(printf 'junk\025one twx\027two three\027\001<\005>!!\033[D\033[D\013\n' | cli_input Text 2>/dev/null)"
check "confirm -p: keys" "yes" "$(printf 'y' | cli_confirm -p Go 2>/dev/null)"
check "confirm: esc is status 3" 3 "$(printf '\033' | status cli_confirm Go y)"

# a wizard: Escape steps back, and the earlier answer is offered again
w_name() { W_NAME=$(cli_input "Name" "${W_NAME:-}"); }
w_fruit() { W_FRUIT=$(cli_select -d "${W_FRUIT:-apple}" "Fruit" apple banana cherry); }
w_save() { W_SAVE=$(cli_confirm -p "Save" y); }
W_NAME='' W_FRUIT='' W_SAVE=''
# name; fruit: back; name: enter keeps it; fruit: banana; save: back; fruit: one further; save: yes
printf 'ana\n\033\n\n\033[B\n\033\n\033[B\ny' >"$HERE/keys.tmp"
cli_wizard w_name w_fruit w_save <"$HERE/keys.tmp" 2>/dev/null
check "wizard: answers after going back twice" "ana cherry yes" "$W_NAME $W_FRUIT $W_SAVE"
printf '\033\n' >"$HERE/keys.tmp"
check "wizard: escape on the first question leaves it" 3 "$(status cli_wizard w_name w_fruit <"$HERE/keys.tmp")"
w_broken() { return 7; }
check "wizard: another failure ends it" 7 "$(status cli_wizard w_broken w_name </dev/null)"
rm -f "$HERE/keys.tmp"
unset CLI_INTERACTIVE
cli_mode plain
W_NAME=''; W_FRUIT=''
cli_wizard w_name w_fruit <<<"bo" 2>/dev/null
check "wizard: without keys it just asks in order" "bo apple" "$W_NAME $W_FRUIT"
cli_mode rich

# ── commands, options, help ──────────────────────────────────────────────────
cli_mode plain
cli_app todo "A tiny todo list" 1.2.0
cli_opt "-v, --verbose" "Say more"
cli_cmd add "Add a task"
cli_args "<text>"
cli_opt "-p, --priority <n>" "1 (high) to 3" 2
cli_opt "--dry-run" "Change nothing"
cli_example 'todo add "Buy milk" -p 1'
cli_cmd list "List tasks" show_list
cli_cmd raw-args "Pass everything through"
cli_cmd fail "Always fails"
cmd_add() { printf 'add text=%s n=%s prio=%s dry=%s verbose=%s\n' "${1:-}" "$#" "$opt_priority" "$opt_dry_run" "$opt_verbose"; }
show_list() { printf 'list %s\n' "$#"; }
cmd_raw_args() { printf 'raw %s\n' "$*"; }
cmd_fail() { return 9; }
# raw-args and list declare no options, but the global -v makes parsing strict for them too.

check "dispatch with defaults" "add text=milk n=1 prio=2 dry= verbose=" "$(cli_run add milk)"
check "value option" "add text=milk n=1 prio=1 dry= verbose=" "$(cli_run add milk -p 1)"
check "long value option" "add text=milk n=1 prio=3 dry= verbose=" "$(cli_run add --priority 3 milk)"
check "option=value" "add text=milk n=1 prio=3 dry= verbose=" "$(cli_run add --priority=3 milk)"
check "flag option" "add text=milk n=1 prio=2 dry=1 verbose=" "$(cli_run add --dry-run milk)"
check "global option after the command" "add text=milk n=1 prio=2 dry= verbose=1" "$(cli_run add milk -v)"
check "global option before the command" "add text=milk n=1 prio=2 dry= verbose=1" "$(cli_run --verbose add milk)"
check "arguments keep their spaces" "add text=buy milk n=2 prio=2 dry= verbose=" "$(cli_run add 'buy milk' 'and eggs')"
check "-- ends the options" "add text=-p n=2 prio=2 dry= verbose=" "$(cli_run add -- -p 1)"
check "no arguments at all" "list 0" "$(cli_run list)"
check "named function" "list 2" "$(cli_run list a b)"
check "dashes in command names" "raw a b" "$(cli_run raw-args a b)"
check "unknown option" "[ERROR] unknown option '--nope' for 'add'" "$(cli_run add --nope 2>&1 | head -1)"
check "unknown option status" 2 "$(status cli_run add --nope)"
check "missing value" "[ERROR] option '-p' needs a value" "$(cli_run add -p 2>&1 || true)"
check "unknown command" "[ERROR] unknown command 'lst'. Did you mean 'list'?" "$(cli_run lst 2>&1 | head -1)"
check "unknown command status" 2 "$(cli_run nope >/dev/null 2>&1 || echo $?)"
check "command status is passed on" 9 "$(cli_run fail || echo $?)"
check "version" "todo 1.2.0" "$(cli_run --version)"
check "defaults reset between runs" "add text=b n=1 prio=2 dry= verbose=" "$(cli_run add a -p 1 --dry-run >/dev/null; cli_run add b)"

help=$(cli_run --help)
has "help: title" "todo 1.2.0 - A tiny todo list" "$help"
has "help: usage" "  todo <command> [options]" "$help"
has "help: commands are aligned" "  add       Add a task${NL}  list      List tasks${NL}  raw-args  Pass everything through" "$help"
has "help: global option" "  -v, --verbose      Say more" "$help"
has "help: builtin options" "  -h, --help         Show help" "$help"
check "no command prints help" "$help" "$(cli_run)"
check "help command" "$help" "$(cli_run help)"
help=$(cli_run help add)
check "command help" "todo add - Add a task${NL}${NL}USAGE${NL}  todo add [options] <text>${NL}${NL}OPTIONS${NL}  -p, --priority <n>  1 (high) to 3 (default: 2)${NL}  --dry-run           Change nothing${NL}  -v, --verbose       Say more${NL}${NL}EXAMPLES${NL}  todo add \"Buy milk\" -p 1" "$help"
check "command --help" "$help" "$(cli_run add --help)"
check "command -h after arguments" "$help" "$(cli_run add milk -h)"
check "help for an unknown command" 2 "$(cli_run help nope >/dev/null 2>&1 || echo $?)"

check "agent help is the whole reference" "todo 1.2.0 - A tiny todo list
usage: todo <command> [args]
commands:
  add <text>  Add a task
    -p, --priority <n>  1 (high) to 3 (default: 2)
    --dry-run  Change nothing
  list  List tasks
  raw-args  Pass everything through
  fail  Always fails
options:
  -v, --verbose  Say more" "$(cli_run --agent help)"
check "agent command help" "todo add <text>  Add a task
  -p, --priority <n>  1 (high) to 3 (default: 2)
  --dry-run  Change nothing
  -v, --verbose  Say more
  e.g. todo add \"Buy milk\" -p 1" "$(cli_run --agent help add)"
check "agent unknown command lists the commands" "error: unknown command 'lst'; commands: add list raw-args fail" "$(cli_run --agent lst 2>&1 || true)"
check "--agent switches the mode" "ok: x" "$(cmd_list_ok() { cli_ok x; }; cli_cmd say "Say" cmd_list_ok; cli_run --agent say 2>&1)"
cli_mode plain

check "interactive mode reads commands from stdin" "list 0${NL}add text=buy milk n=1 prio=1 dry= verbose=${NL}list 2" \
  "$(printf 'list\nadd "buy milk" -p 1\n\nlist a b\nexit\nlist never\n' | cli_run -i 2>/dev/null)"
check "interactive mode survives a failing command" "list 0" "$(printf 'fail\nnope\nlist\n' | cli_run -i 2>/dev/null)"

# interactive mode with keys: completion and history
cli_mode rich
export CLI_INTERACTIVE=1
check "repl: tab completes a command" "list 0" "$(printf 'l\t\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: tab steps through the commands" "list 0" "$(printf '\t\t\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: shift-tab starts from the end" "" "$(printf '\033[Z\nlist\n' | cli_run -i 2>/dev/null)"
check "repl: tab completes an option" "add text=milk n=1 prio=2 dry=1 verbose=" "$(printf 'add milk --d\t\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: tab steps through the options" "add text=milk n=1 prio=2 dry= verbose=1" "$(printf 'add milk -\t\t\t\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: nothing to complete for an argument" "add text=mi n=1 prio=2 dry= verbose=" "$(printf 'add mi\t\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: help completes commands" "todo add - Add a task" "$(printf 'help a\t\nexit\n' | cli_run -i 2>/dev/null | head -1 | strip)"
check "repl: up recalls the last line" "list 1${NL}list 1" "$(printf 'list a\n\033[A\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: up up down" "list 1${NL}list 2${NL}list 2" "$(printf 'list a\nlist a b\n\033[A\033[A\033[B\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: down brings the draft back" "list 1${NL}list 3" "$(printf 'list a\nlist x y z\033[A\033[B\nexit\n' | cli_run -i 2>/dev/null)"
check "repl: the menu row shows the candidates" "add  list  raw-args  fail  help  exit" "$(printf '\t\nexit\n' | cli_run -i 2>&1 >/dev/null | strip | tr '\r' '\n' | sed -n 's/  *$//; /^add  /p' | head -1)"
check "repl: end of input leaves" "list 0" "$(printf 'list\n' | cli_run -i 2>/dev/null)"
unset CLI_INTERACTIVE
cli_mode plain

# a command without any declared option gets its arguments untouched
check "raw arguments" "raw -x --y=1 z" "$("$SH" -c '. "$1"; cli_app t; cli_cmd raw "Raw"; cmd_raw() { printf "raw %s\n" "$*"; }; cli_run raw -x --y=1 z' _ "$LIB")"

# ── the rest ─────────────────────────────────────────────────────────────────
check "has" 0 "$(status cli_has sh)"
check "has not" 1 "$(status cli_has no-such-command-xyz)"
check "require" "[ERROR] missing required command(s): no-such-a no-such-b" "$( (cli_require sh no-such-a no-such-b) 2>&1 || true)"
check "require status" 127 "$(status cli_require sh no-such-a)"
check "require passes" 0 "$(status cli_require sh ls)"
cli_legacy_aliases
check "legacy names" "[OK] yes|ABC|x y" "$(success yes 2>&1)|$(to_upper abc)|$(trim '  x y ')"
check "is_agent" "1 0" "$(status cli_is_agent) $(cli_mode agent; status cli_is_agent)"
check "loading twice is harmless" plain "$(. "$LIB"; cli_mode)"

# ── run as a program ─────────────────────────────────────────────────────────
check "direct call" "- a${NL}- b" "$(CLI_MODE=agent "$SH" "$LIB" list a b)"
check "direct call with the full name" "banana" "$(printf '2\n' | "$SH" "$LIB" cli_select Fruit apple banana 2>/dev/null)"
check "direct call of something unknown" 2 "$("$SH" "$LIB" nope >/dev/null 2>&1 || echo $?)"
has "direct call without arguments lists functions" "  cli_select [-d DEFAULT] QUESTION OPTION..." "$("$SH" "$LIB")"

# ── the examples ─────────────────────────────────────────────────────────────
DEMO="$HERE/../examples/demo.sh"
export DEMO_FAST=1
check "demo: every section, plain" 0 "$(CLI_MODE=plain status "$SH" "$DEMO" all </dev/null)"
check "demo: every section, agent" 0 "$(CLI_MODE=agent status "$SH" "$DEMO" all </dev/null)"
check "demo: every section, rich" 0 "$(CLI_MODE=rich status "$SH" "$DEMO" all </dev/null)"
check "demo: failing release exits 1" 1 "$(CLI_MODE=plain status "$SH" "$DEMO" mock --fail </dev/null)"
has "demo: release to production" "demo-app 2.4.0 is live on production" "$(printf 'y\n' | CLI_MODE=agent "$SH" "$DEMO" mock --env production 2>&1)"
has "demo: production needs a yes" "warn: Stopped before publishing" "$(CLI_MODE=agent "$SH" "$DEMO" mock -e production </dev/null 2>&1)"
has "demo: compare shows three modes" "# layout, agent mode" "$(CLI_MODE=agent "$SH" "$DEMO" compare layout </dev/null 2>&1)"
unset DEMO_FAST

printf '%s: %d passed, %d failed\n' "$SH ${BASH_VERSION:-${ZSH_VERSION:-}}" "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
