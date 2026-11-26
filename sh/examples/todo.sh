#!/usr/bin/env bash
# A small command-line app built on cli.sh: commands, options, help pages, interactive mode.
#
#   examples/todo.sh                 the help page
#   examples/todo.sh add "Buy milk" -p 1
#   examples/todo.sh list
#   examples/todo.sh done            pick the task from a menu
#   examples/todo.sh -i              interactive mode
#   examples/todo.sh --agent help    everything an LLM needs, in a few lines
#
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]:-$0}")/../cli.sh"

FILE=${TODO_FILE:-${TMPDIR:-/tmp}/cli-example-todo.tsv}  # lines of: priority <tab> state <tab> text
T=$'\t'
touch "$FILE"

cli_app todo "A tiny todo list" 1.0.0

cli_cmd add "Add a task"
cli_args "<text>"
cli_opt "-p, --priority <n>" "1 (high) to 3 (low)" 2
cli_example 'todo add "Buy milk" -p 1'

cli_cmd list "Show the tasks"
cli_opt "-a, --all" "Include finished tasks"

cli_cmd done "Mark a task as finished"
cli_args "[number]"

cli_cmd clear "Delete every task"

cmd_add() {
  if [ $# -eq 0 ]; then cli_die 'add needs some text, e.g. todo add "Buy milk"' 2; fi
  printf '%s\t%s\t%s\n' "$opt_priority" open "$*" >>"$FILE"
  cli_ok "Added: $*"
}

cmd_list() {
  local n=0 prio state text
  local -a rows
  rows=("#${T}Task${T}Priority${T}State")
  while IFS=$T read -r prio state text; do
    n=$((n + 1))
    if [ "$state" = done ] && [ -z "$opt_all" ]; then continue; fi
    rows+=("$n$T$text$T$prio$T$state")
  done <"$FILE"
  if [ "${#rows[@]}" -eq 1 ]; then cli_info "Nothing to do."; return 0; fi
  cli_table -s "$T" "${rows[@]}"
}

cmd_done() {
  local pick=${1:-} n=0 prio state text
  local -a open
  open=()
  if [ -z "$pick" ]; then
    while IFS=$T read -r prio state text; do
      n=$((n + 1))
      if [ "$state" = open ]; then open+=("$n. $text"); fi
    done <"$FILE"
    if [ "${#open[@]}" -eq 0 ]; then cli_info "Nothing to do."; return 0; fi
    pick=$(cli_select "Which task is finished?" "${open[@]}")
    pick=${pick%%.*}
  fi
  n=0
  while IFS=$T read -r prio state text; do
    n=$((n + 1))
    if [ "$n" = "$pick" ]; then state=done; cli_ok "Finished: $text"; fi
    printf '%s\t%s\t%s\n' "$prio" "$state" "$text"
  done <"$FILE" >"$FILE.new"
  mv "$FILE.new" "$FILE"
}

cmd_clear() {
  local n
  n=$(($(wc -l <"$FILE")))
  if [ "$n" -eq 0 ]; then cli_info "Nothing to delete."; return 0; fi
  if cli_confirm "Delete all $n tasks?" n; then
    : >"$FILE"
    cli_ok "Deleted $n tasks"
  else
    cli_info "Kept them."
  fi
}

cli_run "$@"
