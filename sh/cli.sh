#!/usr/bin/env bash
# shellcheck shell=bash disable=SC2059,SC2034,SC2154,SC2295
#
# cli.sh — a small, dependency-free toolkit for good-looking shell CLIs.
#
#   . ./cli.sh                       import it (bash 3.2+ or zsh 5.3+)
#   ./cli.sh select "Pick" a b c     or call one function from any language
#   ./cli.sh                         list every function
#
# Three output modes, picked automatically (override: CLI_MODE=…, cli_mode …):
#   rich    a terminal            colours, symbols, animation, key-driven prompts
#   plain   pipes, files, CI      the same words, no escape codes, no animation
#   agent   an LLM is reading     only what is needed, as few tokens as possible
#           (CLI_AGENT=1, --agent, or an agent harness is detected)
#
# Streams:  content (lists, tables, help, answers to prompts)      → stdout
#           status  (info/ok/warn/error, progress, the prompt UI)  → stderr
#
# Environment:
#   CLI_MODE=rich|plain|agent   force a mode          NO_COLOR=1     no colours
#   CLI_MODE_STDERR=...         a mode for standard error alone
#   CLI_AGENT=1|0               force / forbid agent  FORCE_COLOR=1  rich without a tty
#   CLI_QUIET=1                 only warnings+errors  CLI_DEBUG=1    show cli_debug
#   CLI_YES=1                   cli_confirm says yes  CLI_INTERACTIVE=0|1
#   CLI_PAGE=10                 rows shown by a selector               (0: line prompts)
#
# Needs nothing but the shell, plus stty and tput at a terminal, sort for timelines and awk for
# trees of paths.

if [ -n "${_CLI_LOADED:-}" ]; then return 0 2>/dev/null || exit 0; fi
_CLI_LOADED=1
CLI_VERSION=0.1.0

_CLI_US=$'\037' _CLI_NL=$'\n' _CLI_TAB=$'\t' _CLI_ESC=$'\033' _CLI_CR=$'\r'
_CLI_R='' _CLI_KEY=''
_CLI_APP=${0##*/} _CLI_APP_DESC='' _CLI_APP_VER=''
_CLI_CMDS='' _CLI_OPTS='' _CLI_SYN='' _CLI_EX='' _CLI_CUR=@app
_CLI_STEP_I=0 _CLI_STEP_N=0 _CLI_P_T0=''
_CLI_SPLIT='' _CLI_MODE_OUT=plain _CLI_MODE_ERR=plain
_CLI_W='' _CLI_W_AT='' _CLI_LIVE_CLEAR='' _CLI_LIVE_TEXT='' _CLI_K_AT=0
_CLI_K_T0='' _CLI_K_TITLE='' _CLI_K_N=0 _CLI_K_DRAWN=0 _CLI_K_FRAME=0 _CLI_K_Q=0 _CLI_K_DONE=0 _CLI_K_FAILED=0
_CLI_HIST='' _CLI_HIST_N=0 _CLI_EDIT_LIST='' _CLI_STTY='' _CLI_TRAP='' _CLI_UNREAD=''
_CLI_F_N=0 _CLI_F_TOP=0 _CLI_F_FS=0 _CLI_F_DOT=0 _CLI_F_DIRS=0 _CLI_V_N=0
_CLI_ESC_WAIT=1
if [ -n "${ZSH_VERSION:-}" ]; then
  zmodload zsh/datetime 2>/dev/null || true
  _CLI_ESC_WAIT=0.05
elif [ "${BASH_VERSINFO[0]:-3}" -ge 4 ]; then
  _CLI_ESC_WAIT=0.05
fi

# ── modes, colours, symbols ──────────────────────────────────────────────────

_cli_auto() {  # 1|2 → $_CLI_R   the mode for that stream, standard output or standard error
  _CLI_R=plain
  case "${CLI_AGENT:-}" in
    1|true|yes) _CLI_R=agent; return 0;;
    0|false|no) ;;
    *) # environment variables set by common agent harnesses
       if [ -n "${CLAUDECODE:-}${CURSOR_AGENT:-}${GEMINI_CLI:-}${CODEX_SANDBOX:-}${OPENCODE:-}${AI_AGENT:-}${AGENT:-}" ]; then
         _CLI_R=agent; return 0
       fi;;
  esac
  if [ "${TERM:-}" = dumb ]; then return 0; fi
  if [ -n "${FORCE_COLOR:-}" ] && [ "${FORCE_COLOR:-}" != 0 ]; then _CLI_R=rich; return 0; fi
  if [ -t "$1" ]; then _CLI_R=rich; fi
  return 0
}

# Each stream is judged on its own: with `tool > file` the content is plain and the status lines
# at the terminal stay rich. $_CLI_MODE_OUT is for content, $_CLI_MODE_ERR for status lines,
# progress and prompts.
_cli_detect() {
  case "${CLI_MODE:-}" in
    rich|plain|agent) _CLI_MODE_OUT=$CLI_MODE _CLI_MODE_ERR=$CLI_MODE;;
    *) _cli_auto 1; _CLI_MODE_OUT=$_CLI_R; _cli_auto 2; _CLI_MODE_ERR=$_CLI_R;;
  esac
  case "${CLI_MODE_STDERR:-}" in rich|plain|agent) _CLI_MODE_ERR=$CLI_MODE_STDERR;; esac
}

_cli_setup() {
  _cli_detect
  _CLI_MODE=$_CLI_MODE_OUT
  _cli_vars
  if [ "$_CLI_MODE_OUT" != "$_CLI_MODE_ERR" ]; then _cli_split; fi
}

# The colours and symbols below are those of standard output. A function that writes to
# standard error runs under _cli_err, which gives it its own set for as long as it runs.
_cli_err() {  # FUNCTION [ARGS...]
  if [ "$_CLI_MODE" = "$_CLI_MODE_ERR" ]; then "$@"; return; fi
  local CLI_RESET CLI_BOLD CLI_DIM CLI_ITALIC CLI_UNDERLINE CLI_RED CLI_GREEN CLI_YELLOW CLI_BLUE CLI_MAGENTA CLI_CYAN CLI_WHITE CLI_GRAY
  local _CLI_G_OK _CLI_G_ERR _CLI_G_WARN _CLI_G_INFO _CLI_G_DOT _CLI_G_STEP _CLI_G_BULLET _CLI_G_PTR _CLI_G_ON _CLI_G_OFF
  local _CLI_G_FULL _CLI_G_EMPTY _CLI_G_H _CLI_G_V _CLI_G_TL _CLI_G_TR _CLI_G_BL _CLI_G_BR _CLI_G_SPIN _CLI_G_SEP _CLI_G_KEYS
  local _CLI_G_ELL _CLI_G_TT _CLI_G_TB _CLI_G_TLEFT _CLI_G_TRIGHT _CLI_G_TX _CLI_G_NODE _CLI_G_MANY _CLI_G_STOP _CLI_G_SKIP
  local _CLI_MODE=$_CLI_MODE_ERR
  _cli_vars
  "$@"
}

_cli_split() {  # the two streams differ: put every function that writes to standard error under _cli_err
  local f
  if [ -n "$_CLI_SPLIT" ]; then return 0; fi
  _CLI_SPLIT=1
  for f in _cli_msg cli_hint cli_step _cli_exec_show _cli_ps cli_countdown cli_progress cli_progress_done cli_spin \
           _cli_tasks_draw _cli_task_end cli_tasks_done cli_confirm cli_input cli_password cli_pause \
           _cli_pick _cli_pick_line _cli_edit cli_browse; do
    if [ -n "${ZSH_VERSION:-}" ]; then eval "functions[_cli_e_$f]=\${functions[$f]}"
    else eval "$(declare -f "$f" | sed "1s/^$f /_cli_e_$f /")"; fi
    eval "$f() { _cli_err _cli_e_$f \"\$@\"; }"
  done
}

_cli_vars() {  # the colours and symbols that go with $_CLI_MODE
  local e=$'\033[' utf=0
  if [ "$_CLI_MODE" = rich ] && [ -z "${NO_COLOR:-}" ]; then
    CLI_RESET="${e}0m" CLI_BOLD="${e}1m" CLI_DIM="${e}2m" CLI_ITALIC="${e}3m" CLI_UNDERLINE="${e}4m"
    CLI_RED="${e}31m" CLI_GREEN="${e}32m" CLI_YELLOW="${e}33m" CLI_BLUE="${e}34m"
    CLI_MAGENTA="${e}35m" CLI_CYAN="${e}36m" CLI_WHITE="${e}37m" CLI_GRAY="${e}90m"
  else
    CLI_RESET='' CLI_BOLD='' CLI_DIM='' CLI_ITALIC='' CLI_UNDERLINE=''
    CLI_RED='' CLI_GREEN='' CLI_YELLOW='' CLI_BLUE='' CLI_MAGENTA='' CLI_CYAN='' CLI_WHITE='' CLI_GRAY=''
  fi
  case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in *[Uu][Tt][Ff]-8*|*[Uu][Tt][Ff]8*) utf=1;; esac
  if [ "$_CLI_MODE" = rich ] && [ "$utf" = 1 ]; then
    _CLI_G_OK='✓' _CLI_G_ERR='✗' _CLI_G_WARN='▲' _CLI_G_INFO='●' _CLI_G_DOT='·' _CLI_G_STEP='▸'
    _CLI_G_BULLET='•' _CLI_G_PTR='❯' _CLI_G_ON='◉' _CLI_G_OFF='○' _CLI_G_FULL='█' _CLI_G_EMPTY='░'
    _CLI_G_H='─' _CLI_G_V='│' _CLI_G_TL='╭' _CLI_G_TR='╮' _CLI_G_BL='╰' _CLI_G_BR='╯'
    _CLI_G_SPIN='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏' _CLI_G_SEP='›' _CLI_G_KEYS='↑/↓' _CLI_G_ELL='…'
    _CLI_G_TT='┬' _CLI_G_TB='┴' _CLI_G_TLEFT='├' _CLI_G_TRIGHT='┤' _CLI_G_TX='┼' _CLI_G_NODE='●' _CLI_G_MANY='◉' _CLI_G_STOP='■' _CLI_G_SKIP='–'
  else
    _CLI_G_OK='+' _CLI_G_ERR='x' _CLI_G_WARN='!' _CLI_G_INFO='i' _CLI_G_DOT='.' _CLI_G_STEP='>'
    _CLI_G_BULLET='-' _CLI_G_PTR='>' _CLI_G_ON='[x]' _CLI_G_OFF='[ ]' _CLI_G_FULL='#' _CLI_G_EMPTY='-'
    _CLI_G_H='-' _CLI_G_V='|' _CLI_G_TL='+' _CLI_G_TR='+' _CLI_G_BL='+' _CLI_G_BR='+'
    _CLI_G_SPIN='|/-\' _CLI_G_SEP=':' _CLI_G_KEYS='up/down' _CLI_G_ELL='~'
    _CLI_G_TT='+' _CLI_G_TB='+' _CLI_G_TLEFT='+' _CLI_G_TRIGHT='+' _CLI_G_TX='+' _CLI_G_NODE='*' _CLI_G_MANY='#' _CLI_G_STOP='=' _CLI_G_SKIP='-'
  fi
}

cli_mode() {  # cli_mode [-e] [auto|rich|plain|agent]   print the mode (-e: the one of standard error), or set it
  if [ "${1:-}" = -e ]; then printf '%s\n' "$_CLI_MODE_ERR"; return 0; fi
  if [ $# -eq 0 ]; then printf '%s\n' "$_CLI_MODE_OUT"; return 0; fi
  case $1 in
    auto) CLI_MODE='';;
    rich|plain|agent) CLI_MODE=$1 CLI_MODE_STDERR='';;
    *) cli_error "cli_mode: unknown mode '$1' (auto, rich, plain, agent)"; return 2;;
  esac
  export CLI_MODE
  _cli_setup
}

cli_width() {  # cli_width   the terminal's width in columns
  _cli_width; printf '%s\n' "$_CLI_R"
}

cli_is_agent() {  # cli_is_agent   true when output is being read by an LLM
  [ "$_CLI_MODE" = agent ]
}

cli_style() {  # cli_style "bold red" TEXT...   styles: bold dim italic underline inverse, <colour>, bg-<colour>
  local rest="$1 " name code seq=''
  shift
  if [ -z "$CLI_RESET" ]; then printf '%s\n' "$*"; return 0; fi
  while [ -n "$rest" ]; do
    name=${rest%% *}; rest=${rest#* }; code=''
    case $name in
      bold) code=1;; dim) code=2;; italic) code=3;; underline) code=4;; inverse) code=7;;
      black) code=30;; red) code=31;; green) code=32;; yellow) code=33;; blue) code=34;;
      magenta) code=35;; cyan) code=36;; white) code=37;; gray|grey) code=90;;
      bg-black) code=40;; bg-red) code=41;; bg-green) code=42;; bg-yellow) code=43;;
      bg-blue) code=44;; bg-magenta) code=45;; bg-cyan) code=46;; bg-white) code=47;;
    esac
    if [ -n "$code" ]; then seq="$seq${_CLI_ESC}[${code}m"; fi
  done
  printf '%s%s%s\n' "$seq" "$*" "$CLI_RESET"
}

# ── small helpers (results come back in $_CLI_R, so no subshells) ────────────

_cli_rep() {  # CHAR COUNT
  _CLI_R=''
  if [ "$2" -gt 0 ]; then printf -v _CLI_R '%*s' "$2" ''; _CLI_R=${_CLI_R// /$1}; fi
}

_cli_len() {  # TEXT → $_CLI_R   the columns it takes: escape codes none, wide characters (CJK, emoji) two
  case $1 in
    *[!\ -~]*) _cli_len_slow "$1";;  # something other than plain ASCII: look at each character
    *) _CLI_R=${#1};;
  esac
}

_cli_len_slow() {  # TEXT → $_CLI_R   byte by byte, so that it works in every shell
  local LC_ALL=C s=$1 i=0 n=0 b=0 cp=0 more=0 w=0
  n=${#s}
  while [ "$i" -lt "$n" ]; do
    printf -v b '%d' "'${s:$i:1}"; b=$((b & 255)); i=$((i + 1))
    if [ "$b" -eq 27 ]; then  # an escape sequence: ESC [ ... letter
      if [ "${s:$i:1}" = '[' ]; then
        i=$((i + 1))
        while [ "$i" -lt "$n" ]; do
          i=$((i + 1))
          case ${s:$((i - 1)):1} in [a-zA-Z]) break;; esac
        done
      fi
      continue
    fi
    if [ "$b" -lt 128 ]; then
      if [ "$b" -ge 32 ] && [ "$b" -ne 127 ]; then w=$((w + 1)); fi
      continue
    fi
    if [ "$b" -ge 240 ]; then cp=$((b & 7)) more=3
    elif [ "$b" -ge 224 ]; then cp=$((b & 15)) more=2
    elif [ "$b" -ge 192 ]; then cp=$((b & 31)) more=1
    else continue; fi
    while [ "$more" -gt 0 ] && [ "$i" -lt "$n" ]; do
      printf -v b '%d' "'${s:$i:1}"; cp=$(((cp << 6) | (b & 63))); i=$((i + 1)); more=$((more - 1))
    done
    # Marks that sit on the character before them take no room; CJK and most emoji take two.
    if [ $(((cp >= 0x300 && cp <= 0x36F) || (cp >= 0x200B && cp <= 0x200F) || (cp >= 0x20D0 && cp <= 0x20FF) || (cp >= 0xFE00 && cp <= 0xFE0F))) -eq 1 ]; then
      continue
    fi
    if [ $(((cp >= 0x1100 && cp <= 0x115F) || (cp >= 0x2E80 && cp <= 0x303E) || (cp >= 0x3041 && cp <= 0x33FF) || (cp >= 0x3400 && cp <= 0x4DBF) || (cp >= 0x4E00 && cp <= 0x9FFF) || (cp >= 0xA000 && cp <= 0xA4CF) || (cp >= 0xAC00 && cp <= 0xD7A3) || (cp >= 0xF900 && cp <= 0xFAFF) || (cp >= 0xFE30 && cp <= 0xFE4F) || (cp >= 0xFF00 && cp <= 0xFF60) || (cp >= 0xFFE0 && cp <= 0xFFE6) || (cp >= 0x1F300 && cp <= 0x1F64F) || (cp >= 0x1F680 && cp <= 0x1F6FF) || (cp >= 0x1F900 && cp <= 0x1F9FF) || (cp >= 0x20000 && cp <= 0x3FFFD))) -eq 1 ]; then
      w=$((w + 2))
    else
      w=$((w + 1))
    fi
  done
  _CLI_R=$w
}

_cli_cut() {  # TEXT WIDTH → $_CLI_R   as much of TEXT as fits in WIDTH columns; escape codes are dropped
  local s=$1 out='' c='' used=0 i=0 n=0
  case $s in
    *[!\ -~]*) ;;
    *) _CLI_R=${s:0:$2}; return 0;;
  esac
  while :; do
    case $s in
      *"${_CLI_ESC}["*) out="$out${s%%"${_CLI_ESC}["*}"; s=${s#*"${_CLI_ESC}["}; s=${s#*[a-zA-Z]};;
      *) break;;
    esac
  done
  s="$out$s"; out=''; n=${#s}
  while [ "$i" -lt "$n" ]; do
    c=${s:$i:1}; i=$((i + 1))
    _cli_len "$c"
    if [ $((used + _CLI_R)) -gt "$2" ]; then break; fi
    used=$((used + _CLI_R)); out="$out$c"
  done
  _CLI_R=$out
}

_cli_pad() {  # TEXT WIDTH   pad on the right, counting columns, not bytes
  local gap=''
  _cli_len "$1"
  if [ "$_CLI_R" -lt "$2" ]; then printf -v gap '%*s' $(($2 - _CLI_R)) ''; fi
  _CLI_R="$1$gap"
}

_cli_trim() {  # TEXT
  _CLI_R=$1
  _CLI_R=${_CLI_R#"${_CLI_R%%[! ]*}"}
  _CLI_R=${_CLI_R%"${_CLI_R##*[! ]}"}
}

_cli_width() {  # terminal width
  _CLI_R=${COLUMNS:-}
  case $_CLI_R in
    ''|*[!0-9]*)  # asking means running tput: once a second is enough, and a progress bar asks often
      if [ "$_CLI_W_AT" != "${SECONDS:-}" ]; then
        _CLI_W=$(tput cols 2>/dev/null) || _CLI_W=80
        _CLI_W_AT=${SECONDS:-}
      fi
      _CLI_R=$_CLI_W;;
  esac
  case $_CLI_R in ''|*[!0-9]*|0) _CLI_R=80;; esac
}

_cli_now() {  # epoch in milliseconds (whole seconds on shells without a fine clock, e.g. bash 3.2)
  if [ -n "${ZSH_VERSION:-}" ] && [ -n "${EPOCHREALTIME:-}" ]; then
    _CLI_R=$((EPOCHREALTIME * 1000)); _CLI_R=${_CLI_R%.*}
  elif [ -n "${EPOCHREALTIME:-}" ]; then
    _CLI_R=${EPOCHREALTIME/[.,]/}; _CLI_R=$((_CLI_R / 1000))
  else
    _CLI_R=$(($(date +%s) * 1000))
  fi
}

_cli_dur() {  # MILLISECONDS → 340ms, 4.2s, 3m 07s, 1h 02m
  local ms=$1 s
  if [ "$ms" -lt 1000 ]; then
    _CLI_R="${ms}ms"
  elif [ "$ms" -lt 60000 ]; then
    _CLI_R="$((ms / 1000)).$((ms % 1000 / 100))s"
  else
    s=$((ms / 1000))
    if [ "$s" -lt 3600 ]; then printf -v _CLI_R '%dm %02ds' $((s / 60)) $((s % 60))
    else printf -v _CLI_R '%dh %02dm' $((s / 3600)) $((s % 3600 / 60)); fi
  fi
}

_cli_since() {  # START_MS → the time since then, formatted; whole seconds where the clock is coarse
  _cli_now; _CLI_R=$((_CLI_R - $1))
  if [ -z "${EPOCHREALTIME:-}" ] && [ "$_CLI_R" -lt 60000 ]; then _CLI_R="$((_CLI_R / 1000))s"
  else _cli_dur "$_CLI_R"; fi
}

_cli_tty() {  # can we run a key-driven prompt?
  case "${CLI_INTERACTIVE:-}" in 0|false|no) return 1;; 1|true|yes) return 0;; esac
  [ "$_CLI_MODE_ERR" = rich ] && [ -t 0 ] && [ -t 2 ]
}

# ── messages (stderr) ────────────────────────────────────────────────────────

# What is being redrawn in place, a block of tasks or a progress bar, is kept as the text that
# erases it and the text that draws it again, so that a status line can go above it.
_cli_live_off() { if [ -n "$_CLI_LIVE_TEXT" ]; then printf '%s' "$_CLI_LIVE_CLEAR" >&2; fi; }
_cli_live_on() { if [ -n "$_CLI_LIVE_TEXT" ]; then printf '%s' "$_CLI_LIVE_TEXT" >&2; fi; }

_cli_msg() {  # LEVEL TEXT
  local level=$1 text=$2 tag='' color='' glyph=''
  case $level in
    info)  [ -z "${CLI_QUIET:-}" ] || return 0; tag=INFO color=$CLI_BLUE glyph=$_CLI_G_INFO;;
    ok)    [ -z "${CLI_QUIET:-}" ] || return 0; tag=OK color=$CLI_GREEN glyph=$_CLI_G_OK;;
    warn)  tag=WARN color=$CLI_YELLOW glyph=$_CLI_G_WARN;;
    error) tag=ERROR color=$CLI_RED glyph=$_CLI_G_ERR;;
    debug) [ -n "${CLI_DEBUG:-}" ] || return 0; tag=DEBUG color=$CLI_GRAY glyph=$_CLI_G_DOT;;
  esac
  case $_CLI_MODE in
    agent) if [ "$level" = info ]; then printf '%s\n' "$text"; else printf '%s: %s\n' "$level" "$text"; fi;;
    plain) printf '[%s] %s\n' "$tag" "$text";;
    *)     _cli_live_off; printf '%s%s%s %s\n' "$color" "$glyph" "$CLI_RESET" "$text"; _cli_live_on;;
  esac >&2
}

cli_info() {  # cli_info TEXT
  _cli_msg info "$*"
}
cli_ok() {  # cli_ok TEXT   (alias: cli_success)
  _cli_msg ok "$*"
}
cli_success() { _cli_msg ok "$*"; }
cli_warn() {  # cli_warn TEXT
  _cli_msg warn "$*"
}
cli_error() {  # cli_error TEXT
  _cli_msg error "$*"
}
cli_debug() {  # cli_debug TEXT   shown only when CLI_DEBUG=1
  _cli_msg debug "$*"
}
cli_die() {  # cli_die TEXT [EXIT_CODE]   print an error and exit
  _cli_msg error "$1"
  exit "${2:-1}"
}
cli_hint() {  # cli_hint TEXT   a tip for humans; agents never see it
  if [ "$_CLI_MODE" = agent ] || [ -n "${CLI_QUIET:-}" ]; then return 0; fi
  _cli_live_off
  printf '%s%s%s\n' "$CLI_DIM" "$*" "$CLI_RESET" >&2
  _cli_live_on
}

cli_steps() {  # cli_steps TOTAL   start a numbered run of cli_step lines
  _CLI_STEP_N=${1:-0}
  _CLI_STEP_I=0
}
cli_step() {  # cli_step TEXT   "[2/5] TEXT" after cli_steps 5, otherwise "▸ TEXT"
  local pre=''
  if [ -n "${CLI_QUIET:-}" ]; then return 0; fi
  _CLI_STEP_I=$((_CLI_STEP_I + 1))
  if [ "$_CLI_STEP_N" -gt 0 ]; then pre="[$_CLI_STEP_I/$_CLI_STEP_N] "; fi
  case $_CLI_MODE in
    agent) printf '%s%s\n' "$pre" "$*";;
    plain) printf '==> %s%s\n' "$pre" "$*";;
    *)     _cli_live_off
           printf '%s%s%s%s%s%s\n' "$CLI_BOLD$CLI_CYAN" "${pre:-$_CLI_G_STEP }" "$CLI_RESET" "$CLI_BOLD" "$*" "$CLI_RESET"
           _cli_live_on;;
  esac >&2
}

_cli_exec_show() { printf '%s$ %s%s\n' "$CLI_DIM" "$*" "$CLI_RESET" >&2; }

cli_exec() {  # cli_exec COMMAND [ARGS...]   show the command, then run it
  if [ -z "${CLI_QUIET:-}" ]; then _cli_exec_show "$@"; fi
  "$@"
}

# ── layout (stdout) ──────────────────────────────────────────────────────────

cli_nl() {  # cli_nl   a blank line, for humans only
  if [ "$_CLI_MODE" != agent ]; then printf '\n'; fi
}

cli_header() {  # cli_header TITLE
  local title="$*"
  case $_CLI_MODE in
    agent) printf '# %s\n' "$title";;
    plain) printf '\n== %s ==\n' "$title";;
    *)     _cli_rep "$_CLI_G_H" "${#title}"
           printf '\n%s%s%s\n%s%s%s\n' "$CLI_BOLD" "$title" "$CLI_RESET" "$CLI_DIM" "$_CLI_R" "$CLI_RESET";;
  esac
}

cli_hr() {  # cli_hr   a horizontal rule, for humans only
  local w
  if [ "$_CLI_MODE" = agent ]; then return 0; fi
  _cli_width; w=$_CLI_R
  if [ "$w" -gt 80 ]; then w=80; fi
  _cli_rep "$_CLI_G_H" "$w"
  printf '%s%s%s\n' "$CLI_DIM" "$_CLI_R" "$CLI_RESET"
}

cli_box() {  # cli_box [-t TITLE] LINE...   (or lines on stdin) plain text in a frame
  local title='' line='' w=0 inner top
  if [ "${1:-}" = -t ]; then title=$2; shift 2; fi
  if [ $# -eq 0 ] && [ ! -t 0 ]; then
    while IFS= read -r line || [ -n "$line" ]; do set -- "$@" "$line"; done
  fi
  if [ "$_CLI_MODE" = agent ]; then
    if [ -n "$title" ]; then printf '%s:\n' "$title"; fi
    if [ $# -gt 0 ]; then printf '%s\n' "$@"; fi
    return 0
  fi
  for line in "$@"; do _cli_len "$line"; if [ "$_CLI_R" -gt "$w" ]; then w=$_CLI_R; fi; done
  _cli_len "$title"; inner=$_CLI_R
  if [ -n "$title" ] && [ "$w" -lt $((inner + 2)) ]; then w=$((inner + 2)); fi
  if [ -n "$title" ]; then
    _cli_rep "$_CLI_G_H" $((w + 2 - inner - 3)); inner=$((w + 2))
    top="$_CLI_G_H $CLI_RESET$CLI_BOLD$title$CLI_RESET$CLI_GRAY $_CLI_R"
  else
    inner=$((w + 2)); _cli_rep "$_CLI_G_H" "$inner"; top=$_CLI_R
  fi
  printf '%s%s%s%s%s\n' "$CLI_GRAY" "$_CLI_G_TL" "$top" "$_CLI_G_TR" "$CLI_RESET"
  for line in "$@"; do
    _cli_pad "$line" "$w"
    printf '%s%s%s %s %s%s%s\n' "$CLI_GRAY" "$_CLI_G_V" "$CLI_RESET" "$_CLI_R" "$CLI_GRAY" "$_CLI_G_V" "$CLI_RESET"
  done
  _cli_rep "$_CLI_G_H" "$inner"
  printf '%s%s%s%s%s\n' "$CLI_GRAY" "$_CLI_G_BL" "$_CLI_R" "$_CLI_G_BR" "$CLI_RESET"
}

cli_indent() {  # cli_indent [N]   indent stdin by N spaces (default 2)
  local line='' pad
  _cli_rep ' ' "${1:-2}"; pad=$_CLI_R
  while IFS= read -r line || [ -n "$line" ]; do printf '%s%s\n' "$pad" "$line"; done
}

# ── lists, key/values, tables (stdout) ───────────────────────────────────────

cli_list() {  # cli_list [-n] ITEM...   (or items on stdin) bullets; -n numbers them
  local numbered=0 item='' i=0 w
  if [ "${1:-}" = -n ]; then numbered=1; shift; fi
  if [ $# -eq 0 ] && [ ! -t 0 ]; then
    while IFS= read -r item || [ -n "$item" ]; do set -- "$@" "$item"; done
  fi
  w=$#; w=${#w}
  for item in "$@"; do
    i=$((i + 1))
    case $_CLI_MODE:$numbered in
      agent:1) printf '%d. %s\n' "$i" "$item";;
      agent:0) printf -- '- %s\n' "$item";;
      *:1)     printf '  %s%*d.%s %s\n' "$CLI_CYAN" "$w" "$i" "$CLI_RESET" "$item";;
      *)       printf '  %s%s%s %s\n' "$CLI_CYAN" "$_CLI_G_BULLET" "$CLI_RESET" "$item";;
    esac
  done
}

cli_kv() {  # cli_kv KEY VALUE [KEY VALUE]...   aligned "key: value" lines
  local w=0 i=1 k=''
  if [ "$_CLI_MODE" = agent ]; then
    while [ $# -ge 2 ]; do printf '%s: %s\n' "$1" "$2"; shift 2; done
    return 0
  fi
  for k in "$@"; do
    if [ $((i % 2)) -eq 1 ]; then _cli_len "$k"; if [ "$_CLI_R" -gt "$w" ]; then w=$_CLI_R; fi; fi
    i=$((i + 1))
  done
  while [ $# -ge 2 ]; do
    _cli_pad "$1:" $((w + 1))
    printf '  %s%s%s %s\n' "$CLI_DIM" "$_CLI_R" "$CLI_RESET" "$2"
    shift 2
  done
}

_cli_fit() {  # TEXT WIDTH ALIGN(l|r|c)   cut to WIDTH if longer, then pad
  local text=$1 w=$2 gap=0 lead=''
  _cli_len "$text"
  if [ "$_CLI_R" -gt "$w" ]; then _cli_cut "$text" $((w - 1)); text="$_CLI_R$_CLI_G_ELL"; _cli_len "$text"; fi
  gap=$((w - _CLI_R))
  case $3 in
    r) _cli_rep ' ' "$gap"; _CLI_R="$_CLI_R$text";;
    c) _cli_rep ' ' $((gap / 2)); lead=$_CLI_R; _cli_rep ' ' $((gap - gap / 2)); _CLI_R="$lead$text$_CLI_R";;
    *) _cli_rep ' ' "$gap"; _CLI_R="$text$_CLI_R";;
  esac
}

_cli_table_rule() {  # LEFT MIDDLE RIGHT COLUMNS   one horizontal line of a bordered table
  local i=1 w line=$1
  while [ "$i" -le "$4" ]; do
    eval "w=\$_cli_w$i"; _cli_rep "$_CLI_G_H" "$w"; line="$line$_CLI_R"
    if [ "$i" -lt "$4" ]; then line="$line$2"; fi
    i=$((i + 1))
  done
  printf '%s%s%s%s\n' "$CLI_GRAY" "$line" "$3" "$CLI_RESET"
}

cli_table() {  # cli_table [-s SEP] [-a ALIGN] [-k COLUMN]... [-b] [-H] HEADER ROW...   (or rows on stdin) cells split on SEP, default "|"
  # -a lrc      align column 1 left, 2 right, 3 centred (default: numbers right, the rest left)
  # -k COLUMN   never cut this column when the table is too wide: its header, or its number
  # -b          draw borders          -H   no header row: every row is data
  local sep='|' align='' border=0 head=1 row='' rest='' cell='' line='' gap='  ' a='' num='' bar='' keep='' k=''
  local i=0 n=0 r=0 w=0 total=0 over=0 big=0 last=0 rows=0 re='^[-+]?[0-9]+([.,][0-9]+)*%?$'
  while :; do
    case ${1:-} in
      -s) sep=$2; shift 2;;
      -a) align=$2; shift 2;;
      -k) keep="$keep$2$_CLI_NL"; shift 2;;
      -b) border=1; shift;;
      -H) head=0; shift;;
      *) break;;
    esac
  done
  # The rows go into variables and the argument list is emptied: bash copies it on every
  # function call, which makes a long table slow.
  if [ $# -gt 0 ]; then
    for row in "$@"; do rows=$((rows + 1)); eval "_cli_row$rows=\$row"; done
    set --
  elif [ ! -t 0 ]; then
    while IFS= read -r row || [ -n "$row" ]; do rows=$((rows + 1)); eval "_cli_row$rows=\$row"; done
  fi
  if [ "$rows" -eq 0 ]; then return 0; fi

  if [ "$_CLI_MODE" = agent ]; then  # tab-separated, nothing else
    while [ "$r" -lt "$rows" ]; do
      r=$((r + 1)); eval "rest=\$_cli_row$r"; unset "_cli_row$r"; line=''
      while :; do
        _cli_trim "${rest%%"$sep"*}"; line="$line$_CLI_R"
        case $rest in *"$sep"*) rest=${rest#*"$sep"}; line="$line$_CLI_TAB";; *) break;; esac
      done
      printf '%s\n' "$line"
    done
    return 0
  fi

  while [ "$r" -lt "$rows" ]; do  # pass 1: the width of each column, and whether it holds only numbers
    r=$((r + 1)); eval "rest=\$_cli_row$r"; i=0
    while :; do
      i=$((i + 1)); _cli_trim "${rest%%"$sep"*}"; cell=$_CLI_R
      eval "w=\${_cli_w$i:-0} num=\${_cli_num$i:-}"
      _cli_len "$cell"
      if [ "$_CLI_R" -gt "$w" ]; then eval "_cli_w$i=$_CLI_R"; fi
      if [ -n "$cell" ] && [ "$num" != 0 ] && { [ "$r" -gt 1 ] || [ "$head" -eq 0 ]; }; then
        if [[ $cell =~ $re ]]; then eval "_cli_num$i=1"; else eval "_cli_num$i=0"; fi
      fi
      case $rest in *"$sep"*) rest=${rest#*"$sep"};; *) break;; esac
    done
    if [ "$i" -gt "$n" ]; then n=$i; fi
  done
  i=1
  while [ "$i" -le "$n" ]; do
    a=${align:$((i - 1)):1}
    eval "w=\${_cli_w$i:-0} num=\${_cli_num$i:-}"
    case $a in l|r|c) ;; *) if [ "$num" = 1 ]; then a=r; else a=l; fi;; esac
    eval "_cli_w$i=$w _cli_a$i=$a"
    total=$((total + w)); i=$((i + 1))
  done
  while IFS= read -r k; do  # the columns that are never cut: by header first, then by number
    if [ -z "$k" ]; then continue; fi
    big=0
    if [ "$head" -eq 1 ]; then
      rest=$_cli_row1; i=0
      while :; do
        i=$((i + 1)); _cli_trim "${rest%%"$sep"*}"
        if [ "$_CLI_R" = "$k" ]; then big=$i; break; fi
        case $rest in *"$sep"*) rest=${rest#*"$sep"};; *) break;; esac
      done
    fi
    if [ "$big" -eq 0 ]; then
      case $k in *[!0-9]*) ;; *) if [ "$k" -ge 1 ] && [ "$k" -le "$n" ]; then big=$((10#$k)); fi;; esac
    fi
    if [ "$big" -gt 0 ]; then eval "_cli_keep$big=1"; fi
  done <<EOF
$keep
EOF
  if [ "$_CLI_MODE" = rich ]; then  # too wide for the terminal: shorten the widest column that may be cut
    _cli_width
    if [ "$border" -eq 1 ]; then over=$((total + 3 * n + 1 - _CLI_R)); else over=$((total + 2 * n - _CLI_R)); fi
    while [ "$over" -gt 0 ]; do
      i=1; big=0; a=0
      while [ "$i" -le "$n" ]; do
        eval "w=\$_cli_w$i k=\${_cli_keep$i:-}"
        if [ -z "$k" ] && [ "$w" -gt "$a" ]; then big=$i a=$w; fi
        i=$((i + 1))
      done
      if [ "$a" -le 4 ]; then break; fi  # nothing left to cut: the table runs past the edge
      eval "_cli_w$big=$((a - 1))"; over=$((over - 1))
    done
  fi

  bar="$CLI_GRAY$_CLI_G_V$CLI_RESET"
  if [ "$border" -eq 1 ]; then _cli_table_rule "$_CLI_G_TL$_CLI_G_H" "$_CLI_G_H$_CLI_G_TT$_CLI_G_H" "$_CLI_G_H$_CLI_G_TR" "$n"; fi
  r=0
  while [ "$r" -lt "$rows" ]; do  # pass 2: print
    r=$((r + 1)); eval "rest=\$_cli_row$r"; unset "_cli_row$r"; i=0; line=''; last=0
    while :; do
      i=$((i + 1)); cell=''
      if [ "$last" -eq 0 ]; then
        _cli_trim "${rest%%"$sep"*}"; cell=$_CLI_R
        case $rest in *"$sep"*) rest=${rest#*"$sep"};; *) last=1;; esac
      fi
      eval "w=\$_cli_w$i a=\$_cli_a$i"
      _cli_fit "$cell" "$w" "$a"; cell=$_CLI_R
      if [ "$border" -eq 1 ]; then
        if [ "$r" -eq 1 ] && [ "$head" -eq 1 ]; then cell="$CLI_BOLD$cell$CLI_RESET"; fi
        line="$line$bar $cell "
        if [ "$i" -ge "$n" ]; then line="$line$bar"; break; fi
      elif [ "$last" -eq 1 ]; then
        line="$line$cell"; line=${line%"${line##*[! ]}"}; break
      else
        line="$line$cell$gap"
      fi
    done
    if [ "$border" -eq 1 ]; then
      printf '%s\n' "$line"
      if [ "$r" -eq 1 ] && [ "$head" -eq 1 ] && [ "$rows" -gt 1 ]; then
        _cli_table_rule "$_CLI_G_TLEFT$_CLI_G_H" "$_CLI_G_H$_CLI_G_TX$_CLI_G_H" "$_CLI_G_H$_CLI_G_TRIGHT" "$n"
      fi
    elif [ "$r" -gt 1 ] || [ "$head" -eq 0 ]; then
      printf '  %s\n' "$line"
    else
      printf '  %s%s%s\n' "$CLI_BOLD" "$line" "$CLI_RESET"
      i=1; line=''
      while [ "$i" -le "$n" ]; do
        eval "w=\$_cli_w$i"; _cli_rep "$_CLI_G_H" "$w"; line="$line${line:+$gap}$_CLI_R"; i=$((i + 1))
      done
      printf '  %s%s%s\n' "$CLI_DIM" "$line" "$CLI_RESET"
    fi
  done
  if [ "$border" -eq 1 ]; then _cli_table_rule "$_CLI_G_BL$_CLI_G_H" "$_CLI_G_H$_CLI_G_TB$_CLI_G_H" "$_CLI_G_H$_CLI_G_BR" "$n"; fi
  i=1; while [ "$i" -le "$n" ]; do unset "_cli_w$i" "_cli_num$i" "_cli_a$i" "_cli_keep$i"; i=$((i + 1)); done
}

# ── timelines (stdout) ───────────────────────────────────────────────────────

_cli_days() {  # YEAR MONTH DAY → days since 1970-01-01
  local y=$1 m=$2 d=$3 era=0 yoe=0 doy=0
  if [ "$m" -le 2 ]; then y=$((y - 1)); fi
  if [ "$y" -ge 0 ]; then era=$((y / 400)); else era=$(((y - 399) / 400)); fi
  yoe=$((y - era * 400))
  if [ "$m" -gt 2 ]; then doy=$(((153 * (m - 3) + 2) / 5 + d - 1)); else doy=$(((153 * (m + 9) + 2) / 5 + d - 1)); fi
  _CLI_R=$((era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468))
}

_cli_civil() {  # DAYS since 1970-01-01 → $_CLI_Y $_CLI_M $_CLI_D
  local z=$(($1 + 719468)) era=0 doe=0 yoe=0 doy=0 mp=0
  if [ "$z" -ge 0 ]; then era=$((z / 146097)); else era=$(((z - 146096) / 146097)); fi
  doe=$((z - era * 146097))
  yoe=$(((doe - doe / 1460 + doe / 36524 - doe / 146096) / 365))
  doy=$((doe - (365 * yoe + yoe / 4 - yoe / 100)))
  mp=$(((5 * doy + 2) / 153))
  _CLI_D=$((doy - (153 * mp + 2) / 5 + 1))
  if [ "$mp" -lt 10 ]; then _CLI_M=$((mp + 3)); else _CLI_M=$((mp - 9)); fi
  _CLI_Y=$((yoe + era * 400))
  if [ "$_CLI_M" -le 2 ]; then _CLI_Y=$((_CLI_Y + 1)); fi
}

_CLI_RE_DATE='^[0-9]{4}-[0-9]{2}-[0-9]{2}([T ][0-9]{2}:[0-9]{2}(:[0-9]{2})?)?( ?(Z|[-+][0-9]{2}:?[0-9]{2}))?$'
_CLI_RE_INT='^-?[0-9]{1,15}$'

_cli_when() {  # TEXT → $_CLI_R and $_CLI_KIND   date: seconds since 1970; number: the number; order: neither
  local t=$1 s=0
  _CLI_KIND=order
  if [[ $t =~ $_CLI_RE_DATE ]]; then
    case ${t:10:1}${t:13:1} in T:|' :')  # a time of day follows the date
      s=$((10#${t:11:2} * 3600 + 10#${t:14:2} * 60))
      if [ "${t:16:1}" = : ]; then s=$((s + 10#${t:17:2})); fi;;
    esac
    _cli_days $((10#${t:0:4})) $((10#${t:5:2})) $((10#${t:8:2}))
    _CLI_R=$((_CLI_R * 86400 + s)); _CLI_KIND=date
  elif [[ $t =~ $_CLI_RE_INT ]]; then
    case $t in -*) _CLI_R=$((0 - 10#${t#-}));; *) _CLI_R=$((10#$t));; esac
    _CLI_KIND=number
  else
    _CLI_R=0
  fi
}

_cli_field() {  # SEP   take the first cell off $_CLI_REST → $_CLI_R, trimmed
  _cli_trim "${_CLI_REST%%"$1"*}"
  case $_CLI_REST in *"$1"*) _CLI_REST=${_CLI_REST#*"$1"};; *) _CLI_REST='';; esac
}

_cli_tl_lane() {  # NAME → $_CLI_R: the number of the track with that name; a new name is a new track
  local l=1 name=''
  while [ "$l" -le "$_CLI_TL_N" ]; do
    eval "name=\$_cli_tn$l"
    if [ "$name" = "$1" ]; then _CLI_R=$l; return 0; fi
    l=$((l + 1))
  done
  _CLI_TL_N=$l
  eval "_cli_tn$l=\$1"
  _CLI_R=$l
}

_cli_tl_col() {  # POSITION → $_CLI_R: its column on the axis (reads min, max and P of cli_timeline)
  if [ "$max" -eq "$min" ]; then _CLI_R=0
  else _CLI_R=$(((($1 - min) * (P - 1) * 2 + max - min) / (2 * (max - min)))); fi
}

_cli_tl_put() {  # LANE COLUMN CODE   set one cell of a track's line
  local cells=''
  eval "cells=\$_cli_tg$1"
  if [ "${#cells}" -le "$2" ]; then _cli_pad "$cells" "$2"; cells="$_CLI_R$3"
  else cells="${cells:0:$2}$3${cells:$(($2 + 1))}"; fi
  eval "_cli_tg$1=\$cells"
}

_cli_tl_tick() {  # COLUMN TEXT   a label on the axis, unless it would run into the one before or off the end
  if [ "$1" -lt "$next" ] || [ $(($1 + ${#2})) -gt "$P" ]; then return 0; fi
  _cli_pad "$axis" "$1"; axis="$_CLI_R$2"; next=$(($1 + ${#2} + 2))
}

_cli_tl_step() {  # UNIT_SECONDS LABEL_WIDTH STEP...   → $_CLI_R: the first STEP whose labels have room, else the last
  local unit=$1 need=$((($2 + 2) * (max - min)))
  shift 2
  for _CLI_R in "$@"; do
    if [ $((_CLI_R * (P - 1) * unit)) -ge "$need" ]; then return 0; fi
  done
}

cli_timeline() {  # cli_timeline [-v] [-r] [-s SEP] ROW...   (or rows on stdin) ROW is "WHEN|TRACK|LABEL|FROM"
  # one line per TRACK along a time axis        -v   one row per event, one column per track
  # WHEN   2026-03-14, 2026-03-14 09:30, a whole number, or any text (then rows keep their order)
  # FROM   optional: the track this event continues, or merges        -r   with -v: newest first
  local sep='|' vert=0 rev=0 row='' when='' track='' label='' kind=date line='' cells='' g='' c='' tail='' axis=''
  local n=0 L=0 i=0 l=0 a=0 b=0 r=0 lo=0 hi=0 nd=0 ni=0 ww=0 nw=0 p=0 min=0 max=0 W=0 P=0 col=0 next=0
  local left=0 right=0 up=0 down=0 midnight=1 avail=0 t=0 d=0 y=0 m=0 rows=0
  while :; do
    case ${1:-} in
      -s) sep=$2; shift 2;;
      -v) vert=1; shift;;
      -r) rev=1; shift;;
      *) break;;
    esac
  done
  # The rows go into variables and the argument list is emptied: bash copies it on every
  # function call, which makes a long timeline slow.
  if [ $# -gt 0 ]; then
    for row in "$@"; do rows=$((rows + 1)); eval "_cli_tr$rows=\$row"; done
    set --
  elif [ ! -t 0 ]; then
    while IFS= read -r row || [ -n "$row" ]; do rows=$((rows + 1)); eval "_cli_tr$rows=\$row"; done
  fi

  while [ "$i" -lt "$rows" ]; do  # pass 1: the events and where they sit in time
    i=$((i + 1)); eval "_CLI_REST=\$_cli_tr$i"; unset "_cli_tr$i"
    _cli_field "$sep"; when=$_CLI_R
    _cli_field "$sep"; track=$_CLI_R
    _cli_field "$sep"; label=$_CLI_R
    _cli_field "$sep"; g=$_CLI_R
    if [ -z "$when$track$label" ]; then continue; fi
    n=$((n + 1))
    _cli_when "$when"; p=$_CLI_R
    case $_CLI_KIND in date) nd=$((nd + 1));; number) ni=$((ni + 1));; esac
    if [ $((p % 86400)) -ne 0 ]; then midnight=0; fi
    eval "_cli_tw$n=\$when _cli_tx$n=\$label _cli_tl$n=\$track _cli_tf$n=\$g _cli_tp$n=$p _cli_to$n=$n"
    if [ "${#when}" -gt "$ww" ]; then ww=${#when}; fi
    if [ "${#track}" -gt "$nw" ]; then nw=${#track}; fi
  done
  if [ "$n" -eq 0 ]; then return 0; fi
  if [ "$nd" -eq "$n" ]; then kind=date; elif [ "$ni" -eq "$n" ]; then kind=number; else kind=order; fi

  if [ "$kind" = order ]; then  # no dates to go by: the order they were given in
    i=1; while [ "$i" -le "$n" ]; do eval "_cli_tp$i=$i"; i=$((i + 1)); done
  else  # oldest first; events at the same moment keep their order
    line=$(
      i=1
      while [ "$i" -le "$n" ]; do eval "printf '%s\t%s\n' \"\$_cli_tp$i\" $i"; i=$((i + 1)); done |
        LC_ALL=C sort -t "$_CLI_TAB" -k1,1n -k2,2n
    )
    r=0
    while IFS=$_CLI_TAB read -r p i; do r=$((r + 1)); eval "_cli_to$r=$i"; done <<EOF
$line
EOF
  fi
  _CLI_TL_N=0; r=1
  while [ "$r" -le "$n" ]; do  # tracks are numbered in the order they first appear in time
    eval "i=\$_cli_to$r"; eval "track=\$_cli_tl$i g=\$_cli_tf$i"
    _cli_tl_lane "$track"; a=$_CLI_R; b=0
    if [ -n "$g" ] && [ "$g" != "$track" ]; then _cli_tl_lane "$g"; b=$_CLI_R; fi
    eval "_cli_tl$i=$a _cli_tf$i=$b"
    r=$((r + 1))
  done
  L=$_CLI_TL_N

  if [ "$_CLI_MODE" = agent ]; then  # the events in order, tab-separated, nothing drawn
    r=1
    while [ "$r" -le "$n" ]; do
      eval "i=\$_cli_to$r"; eval "a=\$_cli_tl$i b=\$_cli_tf$i when=\$_cli_tw$i label=\$_cli_tx$i"
      line=$when
      if [ "$nw" -gt 0 ]; then eval "track=\$_cli_tn$a"; line="$line$_CLI_TAB$track"; fi
      if [ "$b" -gt 0 ]; then eval "track=\$_cli_tn$b"; line="$line$_CLI_TAB$label${_CLI_TAB}from: $track"
      elif [ -n "$label" ]; then line="$line$_CLI_TAB$label"; fi
      printf '%s\n' "$line"
      r=$((r + 1))
    done
    _cli_tl_free "$n" "$L"
    return 0
  fi

  l=1
  while [ "$l" -le "$L" ]; do
    case $(((l - 1) % 5)) in
      0) c=$CLI_CYAN;; 1) c=$CLI_MAGENTA;; 2) c=$CLI_YELLOW;; 3) c=$CLI_GREEN;; *) c=$CLI_BLUE;;
    esac
    eval "_cli_tc$l=\$c"; l=$((l + 1))
  done

  if [ "$vert" -eq 1 ]; then
    if [ "$rev" -eq 1 ]; then  # newest first
      i=1; r=$n
      while [ "$i" -lt "$r" ]; do
        eval "a=\$_cli_to$i b=\$_cli_to$r"; eval "_cli_to$i=$b _cli_to$r=$a"; i=$((i + 1)); r=$((r - 1))
      done
    fi
    r=1
    while [ "$r" -le "$n" ]; do  # each track's line runs from the first row that involves it to the last
      eval "i=\$_cli_to$r"; eval "a=\$_cli_tl$i b=\$_cli_tf$i"
      eval "_cli_tlo$a=\${_cli_tlo$a:-$r} _cli_thi$a=$r"
      if [ "$b" -gt 0 ]; then eval "_cli_tlo$b=\${_cli_tlo$b:-$r} _cli_thi$b=$r"; fi
      r=$((r + 1))
    done
    avail=0
    if [ "$_CLI_MODE" = rich ]; then  # a label too long for the terminal is cut
      _cli_width; avail=$((_CLI_R - 2 * L - 3))
      if [ "$ww" -gt 0 ]; then avail=$((avail - ww - 2)); fi
      if [ "$nw" -gt 0 ]; then avail=$((avail - nw - 2)); fi
      if [ "$avail" -lt 4 ]; then avail=4; fi
    fi
    r=1
    while [ "$r" -le "$n" ]; do
      eval "i=\$_cli_to$r"; eval "a=\$_cli_tl$i b=\$_cli_tf$i when=\$_cli_tw$i label=\$_cli_tx$i"
      left=$a; right=$a
      if [ "$b" -gt 0 ]; then if [ "$b" -lt "$a" ]; then left=$b; else right=$b; fi; fi
      line=''; l=1
      while [ "$l" -le "$L" ]; do
        eval "lo=\$_cli_tlo$l hi=\$_cli_thi$l c=\$_cli_tc$l"
        if [ "$l" -eq "$a" ]; then g=$_CLI_G_NODE
        elif [ "$l" -eq "$b" ]; then
          up=0; down=0
          if [ "$lo" -lt "$r" ]; then up=1; fi
          if [ "$hi" -gt "$r" ]; then down=1; fi
          if [ "$b" -gt "$a" ]; then
            case $up$down in 11) g=$_CLI_G_TRIGHT;; 10) g=$_CLI_G_BR;; 01) g=$_CLI_G_TR;; *) g=$_CLI_G_H;; esac
          else
            case $up$down in 11) g=$_CLI_G_TLEFT;; 10) g=$_CLI_G_BL;; 01) g=$_CLI_G_TL;; *) g=$_CLI_G_H;; esac
          fi
        elif [ "$l" -gt "$left" ] && [ "$l" -lt "$right" ]; then
          if [ "$lo" -le "$r" ] && [ "$hi" -ge "$r" ]; then g=$_CLI_G_TX; else g=$_CLI_G_H; fi
        elif [ "$lo" -le "$r" ] && [ "$hi" -ge "$r" ]; then g=$_CLI_G_V
        else g=' '; fi
        if [ "$g" = ' ' ]; then line="$line "; else line="$line$c$g$CLI_RESET"; fi
        if [ "$l" -lt "$L" ]; then
          if [ "$l" -ge "$left" ] && [ "$l" -lt "$right" ]; then
            eval "c=\$_cli_tc$b"; line="$line$c$_CLI_G_H$CLI_RESET"
          else line="$line "; fi
        fi
        l=$((l + 1))
      done
      if [ "$avail" -gt 0 ] && [ "${#label}" -gt "$avail" ]; then label="${label:0:$((avail - 1))}$_CLI_G_ELL"; fi
      tail=$label
      if [ "$nw" -gt 0 ]; then
        eval "track=\$_cli_tn$a c=\$_cli_tc$a"
        if [ -n "$tail" ]; then _cli_pad "$track" "$nw"; tail="$c$_CLI_R$CLI_RESET  $tail"
        elif [ -n "$track" ]; then tail="$c$track$CLI_RESET"; fi
      fi
      if [ "$ww" -gt 0 ]; then
        if [ -n "$tail" ]; then _cli_pad "$when" "$ww"; tail="$CLI_DIM$_CLI_R$CLI_RESET  $tail"
        elif [ -n "$when" ]; then tail="$CLI_DIM$when$CLI_RESET"; fi
      fi
      if [ -n "$tail" ]; then line="$line  $tail"; else line=${line%"${line##*[! ]}"}; fi
      printf '  %s\n' "$line"
      r=$((r + 1))
    done
    _cli_tl_free "$n" "$L"
    return 0
  fi

  # across: a time axis, then one line per track
  _cli_width; W=$_CLI_R
  nw=0; l=1
  while [ "$l" -le "$L" ]; do
    eval "track=\$_cli_tn$l"; if [ "${#track}" -gt "$nw" ]; then nw=${#track}; fi; l=$((l + 1))
  done
  i=$((W / 3)); if [ "$i" -lt 8 ]; then i=8; fi
  if [ "$nw" -gt "$i" ]; then nw=$i; fi
  P=$((W - 2)); if [ "$nw" -gt 0 ]; then P=$((P - nw - 2)); fi
  if [ "$P" -lt 10 ]; then P=10; fi
  eval "i=\$_cli_to1"; eval "min=\$_cli_tp$i"
  eval "i=\$_cli_to$n"; eval "max=\$_cli_tp$i"
  r=1
  while [ "$r" -le "$n" ]; do  # each track's line runs from the first column that involves it to the last
    eval "i=\$_cli_to$r"; eval "a=\$_cli_tl$i b=\$_cli_tf$i p=\$_cli_tp$i"
    _cli_tl_col "$p"; col=$_CLI_R
    eval "_cli_tk$i=$col _cli_tlo$a=\${_cli_tlo$a:-$col} _cli_thi$a=$col"
    if [ "$b" -gt 0 ]; then eval "_cli_tlo$b=\${_cli_tlo$b:-$col} _cli_thi$b=$col"; fi
    r=$((r + 1))
  done
  l=1
  while [ "$l" -le "$L" ]; do
    eval "lo=\$_cli_tlo$l hi=\$_cli_thi$l"
    _cli_rep ' ' "$lo"; cells=$_CLI_R; _cli_rep - $((hi - lo + 1)); eval "_cli_tg$l=\$cells\$_CLI_R"
    l=$((l + 1))
  done
  i=1
  while [ "$i" -le "$n" ]; do  # the events; O marks a column that holds more than one
    eval "a=\$_cli_tl$i col=\$_cli_tk$i"; eval "cells=\$_cli_tg$a"
    case ${cells:$col:1} in o|O) _cli_tl_put "$a" "$col" O;; *) _cli_tl_put "$a" "$col" o;; esac
    i=$((i + 1))
  done
  r=1
  while [ "$r" -le "$n" ]; do  # the links between tracks
    eval "i=\$_cli_to$r"; eval "a=\$_cli_tl$i b=\$_cli_tf$i col=\$_cli_tk$i"
    r=$((r + 1))
    if [ "$b" -eq 0 ]; then continue; fi
    eval "cells=\$_cli_tg$b lo=\$_cli_tlo$b hi=\$_cli_thi$b"
    case ${cells:$col:1} in o|O) ;; *)
      up=0; down=0
      if [ "$lo" -lt "$col" ]; then up=1; fi
      if [ "$hi" -gt "$col" ]; then down=1; fi
      if [ "$b" -lt "$a" ]; then
        case $up$down in 11) g=a;; 10) g=b;; 01) g=c;; *) g=d;; esac
      else
        case $up$down in 11) g=e;; 10) g=f;; 01) g=g;; *) g=d;; esac
      fi
      _cli_tl_put "$b" "$col" "$g";;
    esac
    left=$a; right=$b
    if [ "$b" -lt "$a" ]; then left=$b; right=$a; fi
    l=$((left + 1))
    while [ "$l" -lt "$right" ]; do
      eval "cells=\$_cli_tg$l lo=\$_cli_tlo$l hi=\$_cli_thi$l"
      case ${cells:$col:1} in o|O) ;; *)
        if [ "$lo" -le "$col" ] && [ "$hi" -ge "$col" ]; then _cli_tl_put "$l" "$col" x; else _cli_tl_put "$l" "$col" d; fi;;
      esac
      l=$((l + 1))
    done
  done

  eval "i=\$_cli_to1"; eval "when=\$_cli_tw$i"
  case $kind in  # the axis
    order)
      _cli_tl_tick 0 "$when"
      if [ "$n" -gt 1 ]; then eval "i=\$_cli_to$n"; eval "when=\$_cli_tw$i"; _cli_tl_tick $((P - ${#when})) "$when"; fi;;
    number)
      _cli_tl_tick 0 "$min"
      if [ "$max" -ne "$min" ]; then _cli_tl_tick $((P - ${#max})) "$max"; fi;;
    *)
      d=$((min / 86400)); if [ $((min % 86400)) -lt 0 ]; then d=$((d - 1)); fi
      _cli_civil "$d"; y=$_CLI_Y; m=$_CLI_M
      if [ $((max - min)) -ge 63072000 ]; then  # two years or more: years
        _cli_tl_step 31536000 4 1 2 5 10 20 50 100 200 500 1000; i=$_CLI_R
        printf -v g '%04d' "$y"; _cli_tl_tick 0 "$g"
        y=$((y / i * i))
        while :; do
          y=$((y + i)); _cli_days "$y" 1 1; t=$((_CLI_R * 86400))
          if [ "$t" -gt "$max" ]; then break; fi
          _cli_tl_col "$t"; printf -v g '%04d' "$y"; _cli_tl_tick "$_CLI_R" "$g"
        done
      elif [ $((max - min)) -ge 5184000 ]; then  # sixty days or more: months
        _cli_tl_step 2419200 7 1 2 3 6; i=$_CLI_R
        printf -v g '%04d-%02d' "$y" "$m"; _cli_tl_tick 0 "$g"
        while :; do
          m=$((m + 1)); if [ "$m" -gt 12 ]; then m=1; y=$((y + 1)); fi
          _cli_days "$y" "$m" 1; t=$((_CLI_R * 86400))
          if [ "$t" -gt "$max" ]; then break; fi
          if [ $(((m - 1) % i)) -ne 0 ]; then continue; fi
          _cli_tl_col "$t"; printf -v g '%04d-%02d' "$y" "$m"; _cli_tl_tick "$_CLI_R" "$g"
        done
      elif [ $((max - min)) -ge 172800 ] || [ "$midnight" -eq 1 ]; then  # days
        _cli_tl_step 86400 5 1 2 7 14; i=$_CLI_R
        printf -v g '%04d-%02d-%02d' "$y" "$m" "$_CLI_D"; _cli_tl_tick 0 "$g"
        while :; do
          d=$((d + 1)); t=$((d * 86400))
          if [ "$t" -gt "$max" ]; then break; fi
          if [ $(((d % i + i) % i)) -ne 0 ]; then continue; fi
          _cli_civil "$d"; printf -v g '%02d-%02d' "$_CLI_M" "$_CLI_D"
          _cli_tl_col "$t"; _cli_tl_tick "$_CLI_R" "$g"
        done
      else  # under two days: hours
        _cli_tl_step 3600 5 1 2 3 6 12; i=$_CLI_R
        t=$((min - d * 86400))
        printf -v g '%04d-%02d-%02d %02d:%02d' "$y" "$m" "$_CLI_D" $((t / 3600)) $((t % 3600 / 60)); _cli_tl_tick 0 "$g"
        y=$((d * 24 + t / 3600))
        while :; do
          y=$((y + 1)); t=$((y * 3600))
          if [ "$t" -gt "$max" ]; then break; fi
          m=$(((y % 24 + 24) % 24))
          if [ $((m % i)) -ne 0 ]; then continue; fi
          printf -v g '%02d:00' "$m"
          _cli_tl_col "$t"; _cli_tl_tick "$_CLI_R" "$g"
        done
      fi;;
  esac
  if [ -n "$axis" ]; then
    if [ "$nw" -gt 0 ]; then _cli_rep ' ' $((nw + 2)); else _CLI_R=''; fi
    printf '  %s%s%s%s\n' "$_CLI_R" "$CLI_DIM" "$axis" "$CLI_RESET"
  fi
  l=1
  while [ "$l" -le "$L" ]; do
    eval "cells=\$_cli_tg$l c=\$_cli_tc$l track=\$_cli_tn$l"
    cells=${cells//-/$_CLI_G_H}; cells=${cells//o/$_CLI_G_NODE}; cells=${cells//O/$_CLI_G_MANY}; cells=${cells//x/$_CLI_G_TX}
    cells=${cells//a/$_CLI_G_TT}; cells=${cells//b/$_CLI_G_TR}; cells=${cells//c/$_CLI_G_TL}
    cells=${cells//e/$_CLI_G_TB}; cells=${cells//f/$_CLI_G_BR}; cells=${cells//g/$_CLI_G_BL}
    cells=${cells//d/$_CLI_G_V}
    if [ "$nw" -gt 0 ]; then _cli_fit "$track" "$nw" l; cells="$_CLI_R  $cells"; fi
    printf '  %s%s%s\n' "$c" "$cells" "$CLI_RESET"
    l=$((l + 1))
  done
  _cli_tl_free "$n" "$L"
}

_cli_tl_free() {  # EVENTS TRACKS   forget what cli_timeline kept
  local i=1
  while [ "$i" -le "$1" ]; do
    unset "_cli_tw$i" "_cli_tx$i" "_cli_tl$i" "_cli_tf$i" "_cli_tp$i" "_cli_to$i" "_cli_tk$i"; i=$((i + 1))
  done
  i=1
  while [ "$i" -le "$2" ]; do
    unset "_cli_tn$i" "_cli_tc$i" "_cli_tlo$i" "_cli_thi$i" "_cli_tg$i"; i=$((i + 1))
  done
}

# ── trees (stdout) ───────────────────────────────────────────────────────────
#
# A tree is kept as numbered nodes: _cli_fn (name) _cli_fp (parent) _cli_fc (first child)
# _cli_fs (next sibling) _cli_fk (kind: f file, d directory, l link to a directory)
# _cli_fo (open) _cli_fl (entries read) _cli_ft (note). It comes from a list of paths, or from
# a directory, which is read one level at a time and only as far as somebody looks.

# One line per node, parents before their children: PARENT KIND NAME NOTE, separated by \037.
_CLI_F_AWK='
BEGIN { sep = ENVIRON["CLI_SEP"]; n = 0 }
{
  line = $0; note = ""
  at = (sep == "") ? 0 : index(line, sep)
  if (at > 0) {
    note = substr(line, at + length(sep)); line = substr(line, 1, at - 1)
    at = index(note, sep); if (at > 0) note = substr(note, 1, at - 1)
  }
  gsub(/^ +| +$/, "", line); gsub(/^ +| +$/, "", note)
  if (line == "") next
  isdir = (substr(line, length(line)) == "/")
  lead = (substr(line, 1, 1) == "/") ? "/" : ""
  count = split(line, part, "/"); parent = 0; id = 0
  for (i = 1; i <= count; i++) {
    if (part[i] == "" || part[i] == ".") continue
    name = lead part[i]; lead = ""
    key = parent SUBSEP name
    if (!(key in ids)) { n++; ids[key] = n; names[n] = name; parents[n] = parent; kinds[n] = "f"; notes[n] = "" }
    id = ids[key]
    if (parent > 0) kinds[parent] = "d"
    parent = id
  }
  if (id == 0) next
  if (isdir) kinds[id] = "d"
  if (notes[id] == "") notes[id] = note
}
END { for (i = 1; i <= n; i++) printf "%s\037%s\037%s\037%s\n", parents[i], kinds[i], names[i], notes[i] }'

_cli_f_awk() {  # SEP   rows on stdin → node lines
  CLI_SEP=$1 LC_ALL=C awk "$_CLI_F_AWK"
}

_cli_f_build() {  # NODE-LINES   the tree that a list of paths describes
  local p=0 k='' name='' note='' n=0 last=0 top=0
  _CLI_F_N=0 _CLI_F_TOP=0 _CLI_F_FS=0 _CLI_F_DOT=0
  while IFS=$_CLI_US read -r p k name note; do
    if [ -z "$name" ]; then continue; fi
    n=$((n + 1))
    eval "_cli_fn$n=\$name _cli_fp$n=\$p _cli_fc$n=0 _cli_fs$n=0 _cli_fk$n=\$k _cli_fo$n=0 _cli_fl$n=1 _cli_ft$n=\$note _cli_fz$n=0"
    if [ "$p" -eq 0 ]; then
      if [ "$top" -eq 0 ]; then _CLI_F_TOP=$n; else eval "_cli_fs$top=$n"; fi
      top=$n
    else
      eval "last=\$_cli_fz$p"
      if [ "$last" -eq 0 ]; then eval "_cli_fc$p=$n"; else eval "_cli_fs$last=$n"; fi
      eval "_cli_fz$p=$n"
    fi
  done <<EOF
$1
EOF
  _CLI_F_N=$n
}

_cli_f_root() {  # DIR   a tree with one node, the directory; the rest is read when it is looked at
  _CLI_F_N=1 _CLI_F_TOP=1 _CLI_F_FS=1 _CLI_F_DOT=1
  _cli_fn1=$1 _cli_fp1=0 _cli_fc1=0 _cli_fs1=0 _cli_fk1=d _cli_fo1=1 _cli_fl1=0 _cli_ft1=''
}

_cli_f_path() {  # ID → $_CLI_R   the node's path, from the top of the tree
  local i=$1 n=''
  eval "_CLI_R=\$_cli_fn$i i=\$_cli_fp$i"
  while [ "$i" -gt 0 ]; do
    eval "n=\$_cli_fn$i i=\$_cli_fp$i"
    case $n in */) _CLI_R="$n$_CLI_R";; *) _CLI_R="$n/$_CLI_R";; esac
  done
}

_cli_f_load() {  # ID   read a directory's entries, once: directories first, each group in byte order
  local LC_ALL=C id=$1 f='' k='' pass='' last=0 n=$_CLI_F_N dotglob=0 nullglob=0
  if [ "$_CLI_F_FS" != 1 ]; then return 0; fi
  eval "f=\$_cli_fl$id"
  if [ "$f" = 1 ]; then return 0; fi
  eval "_cli_fl$id=1"
  _cli_f_path "$id"; f=$_CLI_R
  if [ -n "${ZSH_VERSION:-}" ]; then
    eval 'set -- "${f%/}"/*(DN)'
  else
    if shopt -q dotglob; then dotglob=1; fi
    if shopt -q nullglob; then nullglob=1; fi
    shopt -s dotglob nullglob
    set -- "${f%/}"/*
    if [ "$dotglob" -eq 0 ]; then shopt -u dotglob; fi
    if [ "$nullglob" -eq 0 ]; then shopt -u nullglob; fi
  fi
  # No function is called in these loops: bash copies the argument list on every call.
  for pass in d f; do
    for f in "$@"; do
      if [ -d "$f" ]; then
        if [ "$pass" = f ]; then continue; fi
        if [ -L "$f" ]; then k=l; else k=d; fi
      else
        if [ "$pass" = d ]; then continue; fi
        k=f
      fi
      n=$((n + 1)); f=${f##*/}
      eval "_cli_fn$n=\$f _cli_fp$n=$id _cli_fc$n=0 _cli_fs$n=0 _cli_fk$n=$k _cli_fo$n=0 _cli_fl$n=0 _cli_ft$n=''"
      if [ "$last" -eq 0 ]; then eval "_cli_fc$id=$n"; else eval "_cli_fs$last=$n"; fi
      last=$n
    done
  done
  _CLI_F_N=$n
}

_cli_f_shown() {  # ID → $_CLI_R   the first node on show among ID and the siblings after it, or 0
  local i=$1 k='' n=''
  while [ "$i" -gt 0 ]; do
    eval "k=\$_cli_fk$i n=\$_cli_fn$i"
    if [ "$_CLI_F_DIRS" = 1 ] && [ "$k" = f ]; then :
    elif [ "$_CLI_F_DOT" = 1 ] && [ "$i" != "$_CLI_F_TOP" ] && [ "${n#.}" != "$n" ]; then :
    else break; fi
    eval "i=\$_cli_fs$i"
  done
  _CLI_R=$i
}

_cli_f_clean() {  # $_CLI_R without control characters, which a file name may hold
  local LC_ALL=C
  _CLI_R=${_CLI_R//[[:cntrl:]]/?}
}

_cli_f_name() {  # NAME KIND → $_CLI_R   the name as shown: a directory ends in "/"
  _CLI_R=$1
  if [ "$_CLI_F_FS" = 1 ]; then case $_CLI_R in *[[:cntrl:]]*) _cli_f_clean;; esac; fi
  if [ "$2" != f ]; then case $_CLI_R in */) ;; *) _CLI_R="$_CLI_R/";; esac; fi
}

_cli_f_walk() {  # MAXDEPTH EVERYTHING(0|1) → the rows on show, $_CLI_V_N of them
  # _cli_vi (node) _cli_vx (the lines drawn before the name) _cli_vd (level) _cli_vm (more inside, not shown)
  # EVERYTHING 1: each directory down to MAXDEPTH, but not through links. 0: the open ones.
  local max=$1 every=$2 i=0 depth=1 r=0 next=0 first=0 k='' o=0 pre='' draw='' sub='' more=0
  local mid='|-- ' end='`-- ' bar='|   '
  if [ "$_CLI_G_V" = '│' ]; then mid='├── ' end='╰── ' bar='│   '; fi
  _cli_f_shown "$_CLI_F_TOP"; i=$_CLI_R
  while [ "$i" -gt 0 ]; do
    eval "next=\$_cli_fs$i k=\$_cli_fk$i o=\$_cli_fo$i"
    _cli_f_shown "$next"; next=$_CLI_R
    if [ "$depth" -eq 1 ]; then
      draw='' sub=''
    else
      eval "pre=\$_cli_vq$depth"
      if [ "$next" -eq 0 ]; then draw="$pre$end" sub="$pre    "; else draw="$pre$mid" sub="$pre$bar"; fi
    fi
    first=0 more=0
    if [ "$k" != f ] && { [ "$every" -eq 1 ] || [ "$o" = 1 ]; }; then
      _cli_f_load "$i"
      eval "first=\$_cli_fc$i"; _cli_f_shown "$first"; first=$_CLI_R
      if [ "$every" -eq 1 ] && [ "$first" -gt 0 ]; then
        if [ "$k" = l ] || { [ "$max" -gt 0 ] && [ "$depth" -ge "$max" ]; }; then more=1 first=0; fi
      fi
    fi
    r=$((r + 1)); eval "_cli_vi$r=$i _cli_vx$r=\$draw _cli_vd$r=$depth _cli_vm$r=$more"
    if [ "$first" -gt 0 ]; then
      eval "_cli_vn$depth=$next"; depth=$((depth + 1)); eval "_cli_vq$depth=\$sub"; i=$first
    else
      i=$next
      while [ "$i" -eq 0 ] && [ "$depth" -gt 1 ]; do depth=$((depth - 1)); eval "i=\$_cli_vn$depth"; done
    fi
  done
  _CLI_V_N=$r
}

_cli_f_free() {  # forget the tree
  local i=1
  while [ "$i" -le $((_CLI_F_N + 1)) ]; do
    unset "_cli_fn$i" "_cli_fp$i" "_cli_fc$i" "_cli_fs$i" "_cli_fk$i" "_cli_fo$i" "_cli_fl$i" "_cli_ft$i" "_cli_fz$i" \
      "_cli_vi$i" "_cli_vx$i" "_cli_vd$i" "_cli_vm$i" "_cli_vq$i" "_cli_vn$i" "_cli_vt$i"
    i=$((i + 1))
  done
  _CLI_F_N=0 _CLI_F_TOP=0 _CLI_V_N=0
}

cli_tree() {  # cli_tree [-d DEPTH] [-s SEP] ROW...   (or rows on stdin) a tree of paths; ROW is "PATH|NOTE"
  # cli_tree [-a] [-d DEPTH] -p DIR   the files under DIR instead        -a   hidden files too
  # -d DEPTH   that many levels; a directory with more inside ends in "…"
  local sep='|' max=0 dir='' all=0 out='' r=0 id=0 draw='' name='' k='' note='' more=0 level=0 w=0 ell='...' text=''
  while :; do
    case ${1:-} in
      -s) sep=$2; shift 2;;
      -d) max=$2; shift 2;;
      -p) dir=$2; shift 2;;
      -a) all=1; shift;;
      *) break;;
    esac
  done
  case $max in ''|*[!0-9]*) max=0;; esac
  _CLI_F_DIRS=0
  if [ -n "$dir" ]; then
    set --
    if [ ! -d "$dir" ]; then cli_error "tree: not a directory: $dir"; return 1; fi
    while :; do case $dir in ?*/) dir=${dir%/};; *) break;; esac; done
    _cli_f_root "$dir"; _CLI_F_DOT=$((1 - all))
    if [ "$max" -gt 0 ]; then max=$((max + 1)); fi  # DEPTH counts the levels under DIR
  else
    if [ $# -gt 0 ]; then out=$(printf '%s\n' "$@" | _cli_f_awk "$sep"); set --
    elif [ ! -t 0 ]; then out=$(_cli_f_awk "$sep"); fi
    _cli_f_build "$out"
  fi
  _cli_f_walk "$max" 1
  if [ "$_CLI_G_ELL" = '…' ]; then ell=$_CLI_G_ELL; fi

  while [ "$r" -lt "$_CLI_V_N" ]; do  # pass 1: the names as shown, and the column the notes start in
    r=$((r + 1)); eval "id=\$_cli_vi$r draw=\$_cli_vx$r more=\$_cli_vm$r"
    eval "name=\$_cli_fn$id k=\$_cli_fk$id note=\$_cli_ft$id"
    _cli_f_name "$name" "$k"; name=$_CLI_R
    eval "_cli_vt$r=\$name"
    if [ -n "$note" ]; then
      level=$((${#draw} + ${#name})); if [ "$more" -eq 1 ]; then level=$((level + 1 + ${#ell})); fi
      if [ "$level" -gt "$w" ]; then w=$level; fi
    fi
  done
  r=0
  while [ "$r" -lt "$_CLI_V_N" ]; do  # pass 2: print
    r=$((r + 1)); eval "id=\$_cli_vi$r draw=\$_cli_vx$r more=\$_cli_vm$r level=\$_cli_vd$r name=\$_cli_vt$r"
    eval "k=\$_cli_fk$id note=\$_cli_ft$id"
    if [ "$_CLI_MODE" = agent ]; then
      _cli_rep ' ' $((level - 1)); text="$_CLI_R$name"
      if [ "$more" -eq 1 ]; then text="$text $ell"; fi
      if [ -n "$note" ]; then text="$text$_CLI_TAB$note"; fi
    else
      level=$((${#draw} + ${#name}))
      text=${draw:+$CLI_DIM$draw$CLI_RESET}
      if [ "$k" = f ]; then text="$text$name"; else text="$text$CLI_BLUE$name$CLI_RESET"; fi
      if [ "$more" -eq 1 ]; then text="$text $CLI_DIM$ell$CLI_RESET"; level=$((level + 1 + ${#ell})); fi
      if [ -n "$note" ]; then _cli_rep ' ' $((w - level)); text="$text$_CLI_R  $CLI_DIM$note$CLI_RESET"; fi
    fi
    printf '%s\n' "$text"
  done
  _cli_f_free
}

# ── time ─────────────────────────────────────────────────────────────────────

cli_now_ms() {  # cli_now_ms   epoch in milliseconds
  _cli_now; printf '%s\n' "$_CLI_R"
}

_cli_size() {  # BYTES → $_CLI_R   512 B, 4.2 KB, 28.0 GB: steps of 1024, one decimal, rounded
  local n=$1 unit=1 name=B next='' tenths=0
  for next in KB MB GB TB PB ''; do
    if [ -z "$next" ] || [ "$n" -lt $((unit * 1024)) ]; then
      if [ "$unit" -eq 1 ]; then _CLI_R="$n B"; return 0; fi
      tenths=$(((n * 10 + unit / 2) / unit))
      if [ "$tenths" -lt 10240 ] || [ -z "$next" ]; then break; fi  # 1024.0 is 1.0 of the next unit
    fi
    unit=$((unit * 1024)); name=$next
  done
  _CLI_R="$((tenths / 10)).$((tenths % 10)) $name"
}

cli_size() {  # cli_size BYTES   → 512 B, 4.2 KB, 28.0 GB
  _cli_size "$1"; printf '%s\n' "$_CLI_R"
}

cli_duration() {  # cli_duration MILLISECONDS   → 340ms, 4.2s, 3m 07s, 1h 02m
  _cli_dur "$1"; printf '%s\n' "$_CLI_R"
}

cli_timer_start() {  # cli_timer_start [NAME]   start (or restart) a stopwatch
  local name=${1:-main}
  name=${name//[^A-Za-z0-9_]/_}
  _cli_now; eval "_CLI_T_$name=\$_CLI_R"
}

cli_timer_ms() {  # cli_timer_ms [NAME]   milliseconds since cli_timer_start
  local name=${1:-main} t0=''
  name=${name//[^A-Za-z0-9_]/_}
  eval "t0=\${_CLI_T_$name:-}"
  if [ -z "$t0" ]; then cli_error "timer '${1:-main}' was never started"; return 1; fi
  _cli_now; printf '%s\n' $((_CLI_R - t0))
}

cli_timer_elapsed() {  # cli_timer_elapsed [NAME]   time since cli_timer_start, formatted
  local name=${1:-main} t0=''
  name=${name//[^A-Za-z0-9_]/_}
  eval "t0=\${_CLI_T_$name:-}"
  if [ -z "$t0" ]; then cli_error "timer '${1:-main}' was never started"; return 1; fi
  _cli_since "$t0"; printf '%s\n' "$_CLI_R"
}

cli_time() {  # cli_time LABEL COMMAND [ARGS...]   run a command and report how long it took
  local label=$1 t0 rc=0
  shift
  _cli_now; t0=$_CLI_R
  "$@" || rc=$?
  _cli_since "$t0"
  _cli_msg info "$label: $_CLI_R"
  return "$rc"
}

cli_countdown() {  # cli_countdown SECONDS [LABEL]   wait, showing the seconds left
  local s=$1 label=${2:-Waiting}
  if [ "$_CLI_MODE" != rich ]; then sleep "$s"; return 0; fi
  while [ "$s" -gt 0 ]; do
    printf '\r%s %s%ds%s\033[K' "$label" "$CLI_BOLD" "$s" "$CLI_RESET" >&2
    sleep 1; s=$((s - 1))
  done
  printf '\r\033[K' >&2
}

# ── progress bar and spinner (stderr) ────────────────────────────────────────

cli_progress() {  # cli_progress CURRENT TOTAL [LABEL]   call repeatedly; finishes itself at TOTAL
  local cur=$1 total=$2 label=${3:-} pct fill q width=24 bar
  if [ "$total" -le 0 ]; then total=1; fi
  if [ "$cur" -gt "$total" ]; then cur=$total; fi
  if [ -z "$_CLI_P_T0" ]; then _cli_now; _CLI_P_T0=$_CLI_R; _CLI_P_Q=-1; _CLI_P_N=0; fi
  _CLI_P_CUR=$cur _CLI_P_TOTAL=$total _CLI_P_LABEL=$label
  pct=$((cur * 100 / total))
  case $_CLI_MODE in
    agent) ;;  # one line, when finished
    plain) q=$((pct / 25))
           if [ "$q" -gt "$_CLI_P_Q" ]; then
             _CLI_P_Q=$q
             printf '%s%d%% (%d/%d)\n' "${label:+$label: }" "$pct" "$cur" "$total" >&2
           fi;;
    *)     fill=$((cur * width / total))
           _cli_rep "$_CLI_G_FULL" "$fill"; bar=$_CLI_R
           _cli_rep "$_CLI_G_EMPTY" $((width - fill)); bar="$CLI_GREEN$bar$CLI_RESET$CLI_DIM$_CLI_R$CLI_RESET"
           # A spinner frame in front, as on every other line that is still running; it turns with
           # each update. The count is padded so that the line does not shift as it grows.
           q=$((_CLI_P_N % ${#_CLI_G_SPIN})); _CLI_P_N=$((_CLI_P_N + 1))
           _cli_since "$_CLI_P_T0"
           printf -v _CLI_LIVE_TEXT '\r%s%s%s %s%s %3d%% %s%*d/%d %s%s\033[K' "$CLI_CYAN" "${_CLI_G_SPIN:$q:1}" "$CLI_RESET" \
             "${label:+$label }" "$bar" "$pct" "$CLI_DIM" "${#total}" "$cur" "$total" "$_CLI_R" "$CLI_RESET"
           _CLI_LIVE_CLEAR="$_CLI_CR${_CLI_ESC}[K"
           printf '%s' "$_CLI_LIVE_TEXT" >&2;;
  esac
  if [ "$cur" -ge "$total" ]; then cli_progress_done; fi
  return 0
}

cli_progress_done() {  # cli_progress_done   end the bar early (a full bar ends itself)
  local text level=ok
  if [ -z "$_CLI_P_T0" ]; then return 0; fi
  _CLI_LIVE_TEXT=''
  case $_CLI_MODE in
    agent) _cli_since "$_CLI_P_T0"
           printf '%s%d/%d %s\n' "${_CLI_P_LABEL:+$_CLI_P_LABEL: }" "$_CLI_P_CUR" "$_CLI_P_TOTAL" "$_CLI_R" >&2;;
    plain) ;;
    *)     # The bar gives way to one line, shaped like the result of a spinner.
           _cli_since "$_CLI_P_T0"
           if [ -n "$_CLI_P_LABEL" ]; then text="$_CLI_P_LABEL ($_CLI_P_CUR/$_CLI_P_TOTAL, $_CLI_R)"
           else text="$_CLI_P_CUR/$_CLI_P_TOTAL ($_CLI_R)"; fi
           if [ "$_CLI_P_CUR" -lt "$_CLI_P_TOTAL" ]; then level=info; fi
           printf '\r\033[K' >&2
           _cli_msg "$level" "$text";;
  esac
  _CLI_P_T0=''
}

# Several tasks under one total. One after the other they are the stages of a job; started
# together they are bars stacked over the total. Each task is kept as _cli_kn (name)
# _cli_kw (weight) _cli_ks (state: w waiting, r running, d done, f failed) _cli_kc (current)
# _cli_kt (total; 0 when it is not counted) _cli_k0 (when it started) _cli_kx (how long it
# took, or why it failed).

cli_tasks() {  # cli_tasks [-s SEP] [-t SECONDS] TITLE [NAME|WEIGHT|SHOW]...   several tasks under one total: name them
  # WEIGHT   how much of the total the task is (default 1): "Scan|1" "Copy|8" "Verify|1"
  # SHOW     words that say what a running task shows: bytes (28.0 GB/68.9 GB instead of plain numbers),
  #          percent (no numbers at all), rate (11.2 MB/s), eta (1h 02m left): "Copy | 8 | bytes rate eta"
  # -t SECONDS   the group started that long ago (when joining work that is already running)
  local sep='|' row='' name='' w='' show='' ago=0
  while :; do
    case ${1:-} in
      -s) sep=$2; shift 2;;
      -t) ago=$2; shift 2;;
      *) break;;
    esac
  done
  if [ -n "$_CLI_K_T0" ]; then cli_tasks_done || true; fi
  _CLI_K_TITLE=${1:-Total}
  if [ $# -gt 0 ]; then shift; fi
  _cli_now; _CLI_K_T0=$((_CLI_R - ago * 1000))
  _CLI_K_N=0 _CLI_K_DRAWN=0 _CLI_K_FRAME=0 _CLI_K_Q=0 _CLI_K_DONE=0 _CLI_K_FAILED=0
  for row in "$@"; do
    _CLI_REST=$row; _cli_field "$sep"; name=$_CLI_R; _cli_field "$sep"; w=$_CLI_R; _cli_field "$sep"; show=$_CLI_R
    case $w in ''|*[!0-9]*) w=1;; esac
    if [ -z "$name" ]; then continue; fi
    _CLI_K_N=$((_CLI_K_N + 1))
    eval "_cli_kn$_CLI_K_N=\$name _cli_kw$_CLI_K_N=$w _cli_ks$_CLI_K_N=w _cli_kc$_CLI_K_N=0 _cli_kt$_CLI_K_N=0 _cli_k0$_CLI_K_N=0 _cli_kx$_CLI_K_N=''"
    eval "_cli_ko$_CLI_K_N=\" \$show \" _cli_kq$_CLI_K_N=''"
  done
}

_cli_task_find() {  # NAME → $_CLI_R   the task's number; a name not seen before joins the group
  local i=1 n=''
  if [ -z "$_CLI_K_T0" ]; then cli_tasks Total; fi
  while [ "$i" -le "$_CLI_K_N" ]; do
    eval "n=\$_cli_kn$i"
    if [ "$n" = "$1" ]; then _CLI_R=$i; return 0; fi
    i=$((i + 1))
  done
  _CLI_K_N=$i
  eval "_cli_kn$i=\$1 _cli_kw$i=1 _cli_ks$i=w _cli_kc$i=0 _cli_kt$i=0 _cli_k0$i=0 _cli_kx$i='' _cli_ko$i=' ' _cli_kq$i=''"
  _CLI_R=$i
}

cli_task() {  # cli_task [-t SECONDS] NAME [CURRENT TOTAL]   a task of the group is running; at TOTAL it is done
  # -t SECONDS   it started that long ago
  local i=0 s='' ago=''
  if [ "${1:-}" = -t ]; then ago=$2; shift 2; fi
  local cur=${2:-0} total=${3:-0}
  _cli_task_find "$1"; i=$_CLI_R
  eval "s=\$_cli_ks$i"
  case $s in
    d|f|s) return 0;;  # what has ended stays ended
    w) _cli_now; eval "_cli_k0$i=$_CLI_R _cli_ks$i=r";;
  esac
  if [ -n "$ago" ]; then _cli_now; eval "_cli_k0$i=$((_CLI_R - ago * 1000))"; fi
  if [ "$s" = w ]; then s=now; else s=soon; fi
  if [ "$total" -gt 0 ] && [ "$cur" -gt "$total" ]; then cur=$total; fi
  eval "_cli_kc$i=$cur _cli_kt$i=$total"
  if [ "$total" -gt 0 ] && [ "$cur" -ge "$total" ]; then _cli_task_end "$i" d ''; return 0; fi
  _cli_tasks_draw "$s"
}

cli_task_done() {  # cli_task_done [-t SECONDS] NAME   a task of the group is done (-t: and it took that long)
  local took=''
  if [ "${1:-}" = -t ]; then _cli_dur $(($2 * 1000)); took=$_CLI_R; shift 2; fi
  _cli_task_find "$1"; _cli_task_end "$_CLI_R" d '' "$took"
}

cli_task_skip() {  # cli_task_skip NAME   a task of the group was not needed; for the total it is done
  _cli_task_find "$1"; _cli_task_end "$_CLI_R" s ''
}

cli_task_note() {  # cli_task_note NAME [TEXT]   a few words next to a task; no TEXT takes them away
  # On a running task they say what it is doing, and go when it ends. On a task that has ended
  # they stay: what came of it.
  local s=''
  _cli_task_find "$1"; eval "_cli_kq$_CLI_R=\${2:-} s=\$_cli_ks$_CLI_R"
  case $s in r) _cli_tasks_draw soon;; d|f|s) _cli_tasks_draw now;; esac
}

cli_task_fail() {  # cli_task_fail NAME [TEXT]   a task of the group failed; TEXT says why
  _cli_task_find "$1"; _cli_task_end "$_CLI_R" f "${2:-}"
}

_cli_task_end() {  # NUMBER d|f|s TEXT [TIME-TAKEN]
  local i=$1 s='' t0=0 name='' took=${4:-}
  eval "s=\$_cli_ks$i t0=\$_cli_k0$i name=\$_cli_kn$i"
  case $s in d|f|s) return 0;; esac
  if [ "$s" = w ]; then _cli_now; t0=$_CLI_R; fi
  if [ -z "$took" ]; then _cli_since "$t0"; took=$_CLI_R; fi
  eval "_cli_kq$i=''"  # what it was doing is over
  if [ "$2" = s ]; then
    eval "_cli_ks$i=s _cli_kx$i=''"
  elif [ "$2" = d ]; then
    eval "_cli_ks$i=d _cli_kx$i=\$took"
    if [ "$_CLI_MODE" = plain ]; then _cli_msg ok "$name ($took)"; fi
  else
    eval "_cli_ks$i=f _cli_kx$i=\$3"
    if [ "$_CLI_MODE" != rich ]; then
      if [ -n "$3" ]; then _cli_msg error "$name: $3"; else _cli_msg error "$name failed"; fi
    fi
  fi
  _cli_tasks_draw now
}

_cli_tasks_draw() {  # now|soon|end   show the group as it stands: at once, not too often, or for the last time
  local end='' how=$1 i=0 n=$_CLI_K_N s='' w=0 c=0 t=0 t0=0 x='' name='' sum=0 W=0 pts=0 pct=0 q=0 waiting=0
  local width=0 nw=${#_CLI_K_TITLE} cw=0 bw=0 fill=0 bar='' line='' out='' lines=0 many=0 frame='' p='' page=${CLI_PAGE:-10}
  local o='' note='' tail='' room=0 now=0 el=0 r=0
  _CLI_K_DONE=0 _CLI_K_FAILED=0
  if [ "$how" = end ]; then end=end; fi
  cw=$((${#n} * 2 + 8))
  while [ "$i" -lt "$n" ]; do
    i=$((i + 1)); eval "s=\$_cli_ks$i w=\$_cli_kw$i c=\$_cli_kc$i t=\$_cli_kt$i name=\$_cli_kn$i o=\$_cli_ko$i note=\$_cli_kq$i"
    pts=0
    if [ "$t" -gt 0 ]; then pts=$((c * 1000 / t)); fi
    case $s in
      d|s) pts=1000; _CLI_K_DONE=$((_CLI_K_DONE + 1));;
      f) _CLI_K_FAILED=$((_CLI_K_FAILED + 1));;
      w) pts=0; waiting=$((waiting + 1));;
    esac
    W=$((W + w)); sum=$((sum + w * pts))
    if [ "${#name}" -gt "$nw" ]; then nw=${#name}; fi
    # Room after the bar for what the task shows, so that the bars keep their length while it changes.
    case $o in
      *' percent '*) r=0;;
      *' bytes '*) _cli_size "$t"; r=$((10 + ${#_CLI_R}));;
      *) r=$((${#t} * 2 + 1));;
    esac
    if [ "$s" = r ]; then
      case $o in *' rate '*) r=$((r + 13));; esac
      case $o in *' eta '*) r=$((r + 13));; esac
      if [ -n "$note" ]; then r=$((r + 2 + ${#note})); fi
    fi
    if [ "$r" -gt "$cw" ]; then cw=$r; fi
  done
  if [ "$W" -eq 0 ]; then W=1; fi
  pct=$((sum / (W * 10)))
  case $_CLI_MODE in
    agent) return 0;;
    plain) q=$((pct / 25))  # a line each time the total passes a quarter
           if [ -z "$end" ] && [ "$q" -gt "$_CLI_K_Q" ] && [ "$q" -lt 4 ]; then
             _CLI_K_Q=$q
             printf '%s: %d%% (%d/%d)\n' "$_CLI_K_TITLE" "$pct" "$_CLI_K_DONE" "$n" >&2
           fi
           return 0;;
  esac

  # A backup of fifty thousand files would otherwise draw fifty thousand times: at a terminal, a
  # task that only moved on is drawn every 80ms at most (where the shell has a clock that fine).
  # Without a terminal (a test, a recording) every step is drawn.
  if [ -n "${EPOCHREALTIME:-}" ]; then
    _cli_now
    if [ "$how" = soon ] && [ -t 2 ] && [ $((_CLI_R - _CLI_K_AT)) -lt 80 ]; then return 0; fi
    _CLI_K_AT=$_CLI_R
  fi
  _cli_width; width=$_CLI_R
  q=$((width / 3)); if [ "$q" -lt 8 ]; then q=8; fi
  if [ "$nw" -gt "$q" ]; then nw=$q; fi
  bw=$((width - nw - 12 - cw))
  if [ "$bw" -gt 24 ]; then bw=24; fi
  if [ "$bw" -lt 4 ]; then bw=4; fi
  case $page in ''|*[!0-9]*|0) page=10;; esac
  if [ "$n" -gt "$page" ]; then many=1; fi  # too many to list: only the ones running or failed
  frame=${_CLI_G_SPIN:$((_CLI_K_FRAME % ${#_CLI_G_SPIN})):1}; _CLI_K_FRAME=$((_CLI_K_FRAME + 1))
  _cli_now; now=$_CLI_R
  i=0
  while [ "$i" -lt "$n" ]; do
    i=$((i + 1)); eval "s=\$_cli_ks$i c=\$_cli_kc$i t=\$_cli_kt$i t0=\$_cli_k0$i x=\$_cli_kx$i name=\$_cli_kn$i o=\$_cli_ko$i note=\$_cli_kq$i"
    if [ "$many" -eq 1 ]; then case $s in w|d|s) continue;; esac; fi
    _cli_fit "$name" "$nw" l; name=$_CLI_R
    case $s in
      w) line="$CLI_DIM$_CLI_G_DOT ${name%"${name##*[! ]}"}$CLI_RESET";;
      d|f|s)
         case $s in
           d) line="$CLI_GREEN$_CLI_G_OK$CLI_RESET $name"; tail=$x;;
           f) line="$CLI_RED$_CLI_G_ERR$CLI_RESET $name"; tail=${x:-failed};;
           *) line="$CLI_DIM$_CLI_G_SKIP$CLI_RESET $name"; tail=skipped;;
         esac
         if [ -n "$note" ]; then tail="$tail  $note"; fi
         room=$((width - nw - 5)); if [ "$room" -lt 1 ]; then room=1; fi
         if [ "${#tail}" -gt "$room" ]; then tail="${tail:0:$((room - 1))}$_CLI_G_ELL"; fi
         line="$line  $CLI_DIM$tail$CLI_RESET";;
      *) # A task still running when the group ends was stopped, and says so.
         if [ -n "$end" ]; then line="$CLI_YELLOW$_CLI_G_STOP$CLI_RESET $name"; else line="$CLI_CYAN$frame$CLI_RESET $name"; fi
         tail=''
         if [ "$t" -gt 0 ]; then
           fill=$((c * bw / t)); printf -v p '%3d' $((c * 100 / t))
           _cli_rep "$_CLI_G_FULL" "$fill"; bar=$_CLI_R
           _cli_rep "$_CLI_G_EMPTY" $((bw - fill)); bar="$CLI_GREEN$bar$CLI_RESET$CLI_DIM$_CLI_R$CLI_RESET"
           line="$line  $bar $p%"; room=$((width - nw - bw - 12))
           case $o in
             *' percent '*) ;;
             *' bytes '*) _cli_size "$c"; tail=$_CLI_R; _cli_size "$t"; tail="$tail/$_CLI_R";;
             *) tail="$c/$t";;
           esac
           el=$((now - t0))
           if [ "$el" -ge 1000 ] && [ "$c" -gt 0 ]; then  # how fast, and how long still: from the average so far
             case $o in *' rate '*)
               r=$((c * 1000 / el))
               case $o in
                 *' bytes '*) _cli_size "$r";;
                 *) if [ "$r" -lt 10 ]; then r=$((c * 10000 / el)); _CLI_R="$((r / 10)).$((r % 10))"; else _CLI_R=$r; fi;;
               esac
               tail="$tail${tail:+  }$_CLI_R/s";;
             esac
             case $o in *' eta '*)
               r=$(((t - c) * (el / 1000) / c))
               if [ "$r" -lt 60 ]; then _CLI_R="${r}s"; else _cli_dur $((r * 1000)); fi
               tail="$tail${tail:+  }$_CLI_R left";;
             esac
           fi
         else
           _cli_since "$t0"; tail=$_CLI_R; room=$((width - nw - 5))
         fi
         if [ -n "$note" ]; then tail="$tail${tail:+  }$note"; fi
         if [ "$room" -lt 1 ]; then room=1; fi
         if [ "${#tail}" -gt "$room" ]; then tail="${tail:0:$((room - 1))}$_CLI_G_ELL"; fi
         if [ -n "$tail" ]; then line="$line  $CLI_DIM$tail$CLI_RESET"; fi;;
    esac
    out="$out$line${_CLI_ESC}[K$_CLI_NL"; lines=$((lines + 1))
  done
  if [ -z "$end" ]; then
    if [ "$many" -eq 1 ]; then
      out="$out  $CLI_DIM$_CLI_K_DONE done, $waiting waiting$CLI_RESET${_CLI_ESC}[K$_CLI_NL"; lines=$((lines + 1))
    fi
    fill=$((sum * bw / (W * 1000))); printf -v p '%3d' "$pct"
    _cli_rep "$_CLI_G_FULL" "$fill"; bar=$_CLI_R
    _cli_rep "$_CLI_G_EMPTY" $((bw - fill)); bar="$CLI_GREEN$bar$CLI_RESET$CLI_DIM$_CLI_R$CLI_RESET"
    _cli_fit "$_CLI_K_TITLE" "$nw" l; name=$_CLI_R
    _cli_since "$_CLI_K_T0"
    out="$out  $CLI_BOLD$name$CLI_RESET  $bar $p%  $CLI_DIM$_CLI_K_DONE/$n $_CLI_R$CLI_RESET${_CLI_ESC}[K$_CLI_NL"; lines=$((lines + 1))
  fi
  if [ "$_CLI_K_DRAWN" -gt 0 ]; then printf '\033[%dA' "$_CLI_K_DRAWN" >&2; fi
  printf '%s\033[J' "$out" >&2
  _CLI_K_DRAWN=$lines
  _CLI_LIVE_TEXT=''
  if [ -z "$end" ] && [ "$lines" -gt 0 ]; then
    _CLI_LIVE_CLEAR="${_CLI_ESC}[${lines}A${_CLI_ESC}[J" _CLI_LIVE_TEXT="$out${_CLI_ESC}[J"
  fi
}

cli_tasks_done() {  # cli_tasks_done   end the group: one line for the whole; status 1 when a task failed
  local level=ok rc=0 i=0
  if [ -z "$_CLI_K_T0" ]; then return 0; fi
  _cli_tasks_draw end
  if [ "$_CLI_K_FAILED" -gt 0 ]; then level=error rc=1
  elif [ "$_CLI_K_DONE" -lt "$_CLI_K_N" ]; then level=info; fi
  _cli_since "$_CLI_K_T0"
  if [ "$_CLI_MODE" = agent ]; then printf '%s: %d/%d %s\n' "$_CLI_K_TITLE" "$_CLI_K_DONE" "$_CLI_K_N" "$_CLI_R" >&2
  else _cli_msg "$level" "$_CLI_K_TITLE ($_CLI_K_DONE/$_CLI_K_N, $_CLI_R)"; fi
  while [ "$i" -lt "$_CLI_K_N" ]; do
    i=$((i + 1)); unset "_cli_kn$i" "_cli_kw$i" "_cli_ks$i" "_cli_kc$i" "_cli_kt$i" "_cli_k0$i" "_cli_kx$i" "_cli_ko$i" "_cli_kq$i"
  done
  _CLI_K_T0='' _CLI_K_N=0 _CLI_K_DRAWN=0
  return "$rc"
}

cli_spin() {  # cli_spin LABEL COMMAND [ARGS...]   spinner while it runs; its output shows only on failure
  local label=$1 out rc=0 spid='' t0 frames i=0 total=0 keep=40
  shift
  if [ -n "${ZSH_VERSION:-}" ]; then setopt localoptions nomonitor nonotify; fi
  out=$(mktemp "${TMPDIR:-/tmp}/cli.XXXXXX") || return 1
  _cli_now; t0=$_CLI_R
  if [ "$_CLI_MODE" = rich ]; then
    (
      frames=$_CLI_G_SPIN; SECONDS=0
      while :; do
        printf '\r%s%s%s %s %s%ds%s\033[K' "$CLI_CYAN" "${frames:$((i % ${#frames})):1}" "$CLI_RESET" \
          "$label" "$CLI_DIM" "$SECONDS" "$CLI_RESET"
        i=$((i + 1)); sleep 0.1
      done
    ) >&2 &
    spid=$!
  elif [ "$_CLI_MODE" = plain ] && [ -z "${CLI_QUIET:-}" ]; then
    printf '[....] %s\n' "$label" >&2
  fi
  "$@" >"$out" 2>&1 || rc=$?
  if [ -n "$spid" ]; then
    kill "$spid" 2>/dev/null || true
    wait "$spid" 2>/dev/null || true
    printf '\r\033[K' >&2
  fi
  _cli_since "$t0"
  if [ "$rc" -eq 0 ]; then
    _cli_msg ok "$label ($_CLI_R)"
  else
    _cli_msg error "$label failed (exit $rc, $_CLI_R)"
    if [ "$_CLI_MODE" = agent ]; then  # an agent gets the tail only
      total=$(($(wc -l <"$out")))
      if [ "$total" -gt "$keep" ]; then printf '(last %d of %d lines)\n' "$keep" "$total" >&2; fi
      tail -n "$keep" "$out" >&2
    else
      cli_indent 2 <"$out" >&2
    fi
  fi
  rm -f "$out"
  return "$rc"
}

# ── prompts (UI on stderr, answer on stdout) ─────────────────────────────────

_cli_getc() {  # [SECONDS]   one character → $_CLI_C; fails at the end of input, or after SECONDS
  local byte=0 more='' rest=0
  _CLI_C=$_CLI_UNREAD
  if [ -n "$_CLI_C" ]; then _CLI_UNREAD=''; return 0; fi
  if [ -n "${ZSH_VERSION:-}" ]; then
    if [ -t 0 ]; then IFS= read -rsk1 ${1:+-t} ${1:+"$1"} _CLI_C || return 1
    else IFS= read -u0 -rk1 ${1:+-t} ${1:+"$1"} _CLI_C || return 1; fi
  else
    IFS= read -rsn1 ${1:+-t} ${1:+"$1"} _CLI_C || return 1
    if [ -z "$_CLI_C" ]; then
      _CLI_C=$_CLI_NL  # bash hands back an empty string for Enter
    elif [ "${BASH_VERSINFO[0]:-3}" -lt 4 ]; then
      # bash 3.2 reads bytes, not characters: fetch the rest of a multi-byte character
      LC_ALL=C printf -v byte '%d' "'$_CLI_C"; byte=$((byte & 255))
      if [ "$byte" -ge 240 ]; then rest=3; elif [ "$byte" -ge 224 ]; then rest=2; elif [ "$byte" -ge 192 ]; then rest=1; fi
      while [ "$rest" -gt 0 ]; do
        IFS= read -rsn1 more || break
        _CLI_C="$_CLI_C$more"; rest=$((rest - 1))
      done
    fi
  fi
}

_cli_key() {  # one keypress → $_CLI_KEY: the character, or a name (up, enter, tab, backtab, esc, ...)
  local seq=''
  _CLI_KEY=''
  _cli_getc || return 1
  case $_CLI_C in
    "$_CLI_ESC")  # alone it is Escape; followed by more it is an arrow or another special key
      if ! _cli_getc "$_CLI_ESC_WAIT" || [ "$_CLI_C" = "$_CLI_NL" ]; then _CLI_KEY=esc; return 0; fi
      seq=$_CLI_C
      case $seq in '['|O) ;; *) _CLI_UNREAD=$seq; _CLI_KEY=esc; return 0;; esac  # Escape, then an ordinary key
      case $seq in '['|O)
        if _cli_getc "$_CLI_ESC_WAIT"; then
          seq="$seq$_CLI_C"
          while :; do
            case $_CLI_C in [0-9\;]) _cli_getc "$_CLI_ESC_WAIT" || break; seq="$seq$_CLI_C";; *) break;; esac
          done
        fi;;
      esac
      case $seq in
        '[A'|OA) _CLI_KEY=up;;
        '[B'|OB) _CLI_KEY=down;;
        '[C'|OC) _CLI_KEY=right;;
        '[D'|OD) _CLI_KEY=left;;
        '[Z') _CLI_KEY=backtab;;
        '[H'|OH|'[1~'|'[7~') _CLI_KEY=home;;
        '[F'|OF|'[4~'|'[8~') _CLI_KEY=end;;
        '[3~') _CLI_KEY=delete;;
        *) _CLI_KEY=other;;
      esac;;
    "$_CLI_NL"|$'\r') _CLI_KEY=enter;;
    ' ') _CLI_KEY=space;;
    $'\t') _CLI_KEY=tab;;
    $'\177'|$'\b') _CLI_KEY=backspace;;
    $'\001') _CLI_KEY=home;;
    $'\005') _CLI_KEY=end;;
    $'\025') _CLI_KEY=kill-left;;
    $'\013') _CLI_KEY=kill-right;;
    $'\027') _CLI_KEY=kill-word;;
    $'\003') if [ -n "$_CLI_STTY" ]; then _cli_interrupted; return 1; fi; _CLI_KEY=other;;
    $'\004') _CLI_KEY=eof;;
    *) _CLI_KEY=$_CLI_C;;
  esac
}

# Reading keys changes how the terminal behaves, and Ctrl-C must never leave it that way.
# zsh: we switch the terminal ourselves, with Ctrl-C delivered as a key, and switch it back.
# bash: `read -n1` switches it for each key but, interrupted, may not switch it back (3.2 does
# not), so we remember the state and restore it from a SIGINT trap.
_cli_raw_on() {
  _CLI_STTY=''
  if [ ! -t 0 ]; then return 0; fi
  _CLI_STTY=$(stty -g 2>/dev/null) || _CLI_STTY=''
  if [ -z "$_CLI_STTY" ]; then return 0; fi
  if [ -n "${ZSH_VERSION:-}" ]; then
    stty -echo -icanon -isig min 1 time 0 2>/dev/null || true
  else
    _CLI_TRAP=$(trap -p INT)
    trap _cli_interrupted INT
  fi
}

_cli_raw_off() {
  if [ -z "$_CLI_STTY" ]; then return 0; fi
  stty "$_CLI_STTY" 2>/dev/null || true
  _CLI_STTY=''
  if [ -z "${ZSH_VERSION:-}" ]; then
    if [ -n "$_CLI_TRAP" ]; then eval "$_CLI_TRAP"; else trap - INT; fi
  fi
}

_cli_interrupted() {  # Ctrl-C: give the terminal back, then let the interrupt do what it would have done
  _cli_raw_off
  printf '\n' >&2
  kill -INT 0
}

_cli_complete_list() {  # the candidates of cli_input -c
  _CLI_R=$_CLI_EDIT_LIST
}

_cli_complete_app() {  # TEXT-BEFORE-CURSOR   what could come next on a command line of this app
  local text=$1 first name fn desc o var takes short long flags def
  _CLI_R=''
  text=${text#"${text%%[! ]*}"}
  first=${text%% *}
  case $text in
    *' '*) ;;
    *) # the first word: a command
      while IFS=$_CLI_US read -r name fn desc; do
        if [ -n "$name" ]; then _CLI_R="$_CLI_R$name$_CLI_NL"; fi
      done <<<"$_CLI_CMDS"
      _CLI_R="${_CLI_R}help${_CLI_NL}exit$_CLI_NL"
      return 0;;
  esac
  if [ "$first" = help ]; then
    while IFS=$_CLI_US read -r name fn desc; do
      if [ -n "$name" ]; then _CLI_R="$_CLI_R$name$_CLI_NL"; fi
    done <<<"$_CLI_CMDS"
    return 0
  fi
  case ${text##* } in -*) ;; *) return 0;; esac  # after a command only options can be completed
  for name in "$first" @app; do  # the command's own options first, then the global ones
    while IFS=$_CLI_US read -r o var takes short long flags def desc; do
      if [ "$o" != "$name" ]; then continue; fi
      if [ "$long" != - ]; then _CLI_R="$_CLI_R$long$_CLI_NL"; else _CLI_R="$_CLI_R$short$_CLI_NL"; fi
    done <<<"$_CLI_OPTS"
  done
  _CLI_R="${_CLI_R}--help$_CLI_NL"
}

_cli_edit() {  # PROMPT line|value   a one-line editor → $_CLI_R. Status 1: end of input, 3: Escape.
  # line:  a command line. Tab completes the word under the cursor, up/down walk the history.
  # value: one answer. Tab completes the whole text from _CLI_EDIT_LIST, Escape cancels.
  # Tab and Shift-Tab step forwards and backwards through the candidates, shown underneath.
  local ps=$1 kind=$2 left='' right='' key='' c='' stem='' word='' cands='' cand='' menu='' draft=''
  local n=0 ci=0 i=0 hi=0 maxw=0 used=0
  _CLI_R=''
  _cli_width; maxw=$((_CLI_R - 2)); _CLI_R=''
  _cli_raw_on
  while :; do
    menu=''
    if [ "$ci" -gt 0 ] && [ "$n" -gt 1 ]; then  # the row of candidates, as much as fits
      i=0; used=0
      while IFS= read -r cand; do
        if [ -z "$cand" ]; then continue; fi
        i=$((i + 1))
        if [ $((used + ${#cand} + 2)) -gt "$maxw" ]; then
          if [ "$i" -gt "$ci" ]; then menu="$menu$CLI_DIM$_CLI_G_ELL$CLI_RESET"; break; fi
          menu="$CLI_DIM$_CLI_G_ELL$CLI_RESET  "; used=3
        fi
        if [ "$i" -eq "$ci" ]; then menu="$menu$CLI_BOLD$CLI_CYAN$cand$CLI_RESET  "
        else menu="$menu$CLI_DIM$cand$CLI_RESET  "; fi
        used=$((used + ${#cand} + 2))
      done <<<"$cands"
      menu="$_CLI_NL$menu${_CLI_ESC}[1A$_CLI_CR"  # below the line, then back up to it
    fi
    printf '\r\033[J%s%s%s%s' "$menu" "$ps" "$left" "$right" >&2
    if [ -n "$right" ]; then printf '\033[%dD' "${#right}" >&2; fi

    if ! _cli_key; then  # the end of the input
      if [ -n "$left$right" ]; then break; fi
      _cli_raw_off; printf '\n' >&2
      return 1
    fi
    key=$_CLI_KEY
    case $key in
      tab|backtab) ;;
      esc) if [ "$ci" -gt 0 ]; then left="${left%"$word"}$stem"; ci=0; cands=''; continue; fi;;
      *) ci=0; cands='';;
    esac
    case $key in
      enter) break;;
      tab|backtab)
        if [ "$ci" -eq 0 ]; then
          if [ "$kind" = line ]; then stem=${left##* }; _cli_complete_app "$left"; else stem=$left; _cli_complete_list; fi
          word=$stem; cands=''; n=0
          while IFS= read -r cand; do
            case $cand in '') ;; "$stem"*) cands="$cands$cand$_CLI_NL"; n=$((n + 1));; esac
          done <<<"$_CLI_R"
          if [ "$n" -eq 0 ]; then continue; fi
          if [ "$key" = tab ]; then ci=1; else ci=$n; fi
        elif [ "$key" = tab ]; then ci=$((ci % n + 1))
        else ci=$(((ci + n - 2) % n + 1)); fi
        i=0
        while IFS= read -r cand; do
          i=$((i + 1)); if [ "$i" -eq "$ci" ]; then break; fi
        done <<<"$cands"
        left="${left%"$word"}$cand"; word=$cand
        if [ "$n" -eq 1 ]; then  # only one: take it
          if [ "$kind" = line ]; then left="$left "; fi
          ci=0; cands=''
        fi;;
      esc)
        if [ "$kind" = value ]; then
          _cli_raw_off; printf '\r\033[J%s%s%s\n' "$ps" "$left" "$right" >&2
          return 3
        fi;;
      backspace) left=${left%?};;
      delete) right=${right#?};;
      left) if [ -n "$left" ]; then c=${left#"${left%?}"}; left=${left%?}; right="$c$right"; fi;;
      right) if [ -n "$right" ]; then c=${right%"${right#?}"}; right=${right#?}; left="$left$c"; fi;;
      home) right="$left$right"; left='';;
      end) left="$left$right"; right='';;
      kill-left) left='';;
      kill-right) right='';;
      kill-word) left=${left%"${left##*[! ]}"}; left=${left%"${left##* }"};;
      up|down)
        if [ "$kind" != line ]; then continue; fi
        if [ "$key" = up ] && [ "$hi" -lt "$_CLI_HIST_N" ]; then
          if [ "$hi" -eq 0 ]; then draft="$left$right"; fi
          hi=$((hi + 1))
        elif [ "$key" = down ] && [ "$hi" -gt 0 ]; then hi=$((hi - 1))
        else continue; fi
        left=$draft; right=''
        if [ "$hi" -gt 0 ]; then
          i=0
          while IFS= read -r cand; do
            i=$((i + 1)); if [ "$i" -eq $((_CLI_HIST_N - hi + 1)) ]; then left=$cand; break; fi
          done <<<"$_CLI_HIST"
        fi;;
      eof) if [ -z "$left$right" ]; then _cli_raw_off; printf '\n' >&2; return 1; fi;;
      space) left="$left ";;
      ?) case $key in [[:cntrl:]]) ;; *) left="$left$key";; esac;;
    esac
  done
  _cli_raw_off
  _CLI_R="$left$right"
  printf '\r\033[J%s%s\n' "$ps" "$_CLI_R" >&2
}

_cli_readline() {  # one line of stdin; fails when there is none. Agent mode never blocks.
  _CLI_R=''
  if [ "$_CLI_MODE" = agent ]; then
    if [ -t 0 ]; then return 1; fi
    IFS= read -r -t 1 _CLI_R || [ -n "$_CLI_R" ]
  else
    IFS= read -r _CLI_R || [ -n "$_CLI_R" ]
  fi
}

cli_confirm() {  # cli_confirm [-p] QUESTION [y|n]   status 0 yes, 1 no, 2 nobody answered, 3 Escape
  # -p   print "yes" or "no" instead, like the other prompts (status 0 when answered)
  local print=0 ans='' hint='[y/n]' rc=0
  if [ "${1:-}" = -p ]; then print=1; shift; fi
  local q=$1 def=${2:-}
  case $def in y|Y|yes) def=y hint='[Y/n]';; n|N|no) def=n hint='[y/N]';; *) def='';; esac
  if [ -n "${CLI_YES:-}" ]; then
    ans=y
  elif _cli_tty; then
    printf '%s?%s %s %s%s%s ' "$CLI_CYAN" "$CLI_RESET" "$q" "$CLI_DIM" "$hint" "$CLI_RESET" >&2
    _cli_raw_on
    while [ -z "$ans" ]; do
      _cli_key || { ans=${def:-n}; break; }
      case $_CLI_KEY in y|Y) ans=y;; n|N) ans=n;; esc) ans=n; rc=3;; enter) ans=$def;; esac
    done
    _cli_raw_off
    if [ "$ans" = y ]; then printf 'yes\n' >&2; else printf 'no\n' >&2; fi
    if [ "$rc" -eq 3 ]; then return 3; fi
  else
    if [ "$_CLI_MODE" != agent ]; then printf '%s %s ' "$q" "$hint" >&2; fi
    _cli_readline || true
    case $_CLI_R in y|Y|yes|Yes|YES) ans=y;; n|N|no|No|NO) ans=n;; '') ans=$def;; esac
    if [ "$_CLI_MODE" = agent ]; then
      if [ -n "$ans" ]; then printf '%s %s\n' "$q" "$ans" >&2; fi
    elif [ ! -t 0 ]; then
      printf '%s\n' "$ans" >&2
    fi
    if [ -z "$ans" ]; then
      cli_error "input required: $q [y/n]. Pipe y or n on stdin, or set CLI_YES=1."
      return 2
    fi
  fi
  if [ "$print" -eq 1 ]; then
    if [ "$ans" = y ]; then printf 'yes\n'; else printf 'no\n'; fi
    return 0
  fi
  [ "$ans" = y ]
}

cli_input() {  # cli_input [-c CANDIDATE]... QUESTION [DEFAULT]   → the answer on stdout
  # -c   a suggestion; Tab and Shift-Tab step through the ones that match what was typed
  local cands='' hints='' ans='' got=0 rc=0
  while [ "${1:-}" = -c ]; do cands="$cands$2$_CLI_NL"; hints="$hints${hints:+ | }$2"; shift 2; done
  local q=$1 def=${2:-}
  if _cli_tty; then
    _CLI_EDIT_LIST=$cands
    _cli_edit "$CLI_CYAN?$CLI_RESET $q${def:+ $CLI_DIM($def)$CLI_RESET} " value || rc=$?
    if [ "$rc" -eq 3 ]; then return 3; fi
    ans=$_CLI_R
    if [ "$rc" -eq 0 ]; then got=1; fi
  else
    if [ "$_CLI_MODE" != agent ]; then printf '%s%s: ' "$q" "${def:+ [$def]}" >&2; fi
    if _cli_readline; then got=1; fi
    ans=$_CLI_R
    if [ "$_CLI_MODE" != agent ] && [ ! -t 0 ]; then printf '%s\n' "${ans:-$def}" >&2; fi
  fi
  ans=${ans:-$def}
  if [ -z "$ans" ] && [ "$got" -eq 0 ]; then
    cli_error "input required: $q${hints:+ (suggestions: $hints)}. Pipe the answer on stdin."
    return 1
  fi
  if [ "$_CLI_MODE" = agent ]; then printf '%s: %s\n' "$q" "$ans" >&2; fi
  printf '%s\n' "$ans"
}

cli_password() {  # cli_password QUESTION   → the answer on stdout; typing is hidden
  local q=$1 ans=''
  # At a terminal the typing is hidden whatever the mode: in `tool export | tar x` the output
  # is a pipe, so the mode is plain, and a person is still typing the password.
  if _cli_tty || { [ -t 0 ] && [ "$_CLI_MODE" != agent ]; }; then
    # Echo goes off before the question shows, and stays off between keys: what is typed the
    # moment the question appears, or pasted in one go, stays hidden.
    _cli_raw_on
    if [ -n "$_CLI_STTY" ]; then stty -echo 2>/dev/null || true; fi
    printf '%s?%s %s ' "$CLI_CYAN" "$CLI_RESET" "$q" >&2
    while _cli_key; do
      case $_CLI_KEY in
        enter|eof) break;;
        backspace) ans=${ans%?};;
        kill-left) ans='';;
        space) ans="$ans ";;
        ?) ans="$ans$_CLI_KEY";;
      esac
    done
    _cli_raw_off
    printf '\n' >&2
  else
    _cli_readline || { cli_error "input required: $q. Pipe the answer on stdin."; return 1; }
    ans=$_CLI_R
  fi
  printf '%s\n' "$ans"
}

cli_pause() {  # cli_pause [TEXT]   wait for a key; does nothing without a terminal
  _cli_tty || return 0
  printf '%s%s%s' "$CLI_DIM" "${1:-Press any key to continue}" "$CLI_RESET" >&2
  _cli_raw_on
  _cli_key || true
  _cli_raw_off
  printf '\r\033[K' >&2
}

cli_wizard() {  # cli_wizard FUNCTION...   ask questions in order; Escape goes back to the one before
  # Each FUNCTION asks one question and keeps the answer, e.g.
  #   ask_env() { env=$(cli_select -d "${env:-staging}" "Deploy to" staging production); }
  # Status 3 from a question (Escape) steps back; any other failure ends the wizard with it.
  local i=1 n=$# rc=0
  while [ "$i" -le "$n" ]; do
    rc=0
    eval "\"\${$i}\"" || rc=$?
    case $rc in
      0) i=$((i + 1));;
      3) if [ "$i" -eq 1 ]; then return 3; fi; i=$((i - 1));;
      *) return "$rc";;
    esac
  done
}

cli_select() {  # cli_select [-d DEFAULT] QUESTION OPTION...   → the chosen option on stdout
  local def=''
  if [ "${1:-}" = -d ]; then def=$2; shift 2; fi
  _cli_pick single "$1" "$def" "${@:2}"
}

cli_multiselect() {  # cli_multiselect [-d DEFAULT]... QUESTION OPTION...   → chosen options, one per line
  local def=''
  while [ "${1:-}" = -d ]; do def="$def${def:+$_CLI_NL}$2"; shift 2; done
  _cli_pick multi "$1" "$def" "${@:2}"
}

_cli_pick() {  # single|multi QUESTION DEFAULTS OPTION...
  local kind=$1 title=$2 defaults=$3
  shift 3
  local n=$# cur=1 top=1 page=${CLI_PAGE:-10} marks=' ' i=1 last=0 drawn=0 maxw=0
  local item='' mark='' hint='' out='' shown=''
  if [ "$n" -eq 0 ]; then cli_error "$title: no options to choose from"; return 2; fi
  if ! _cli_tty; then _cli_pick_line "$kind" "$title" "$defaults" "$@"; return; fi

  for item in "$@"; do
    case "$_CLI_NL$defaults$_CLI_NL" in *"$_CLI_NL$item$_CLI_NL"*)
      if [ "$kind" = multi ]; then marks="$marks$i "; else cur=$i; fi;;
    esac
    i=$((i + 1))
  done
  if [ "$page" -gt "$n" ]; then page=$n; fi
  _cli_width; maxw=$((_CLI_R - 8))
  hint="$_CLI_G_KEYS move, enter select"
  if [ "$kind" = multi ]; then hint="$_CLI_G_KEYS move, space toggle, a all, enter confirm"; fi
  printf '%s?%s %s%s%s  %s%s%s\n' "$CLI_CYAN" "$CLI_RESET" "$CLI_BOLD" "$title" "$CLI_RESET" "$CLI_DIM" "$hint" "$CLI_RESET" >&2

  _cli_raw_on
  while :; do
    if [ "$cur" -lt "$top" ]; then top=$cur; fi
    if [ "$cur" -ge $((top + page)) ]; then top=$((cur - page + 1)); fi
    i=$top; last=$((top + page - 1))
    while [ "$i" -le "$last" ]; do
      eval "item=\${$i}"
      item=${item:0:$maxw}; mark=''
      if [ "$kind" = multi ]; then
        case $marks in
          *" $i "*) mark="$CLI_GREEN$_CLI_G_ON$CLI_RESET ";;
          *)        mark="$CLI_DIM$_CLI_G_OFF$CLI_RESET ";;
        esac
      fi
      if [ "$i" -eq "$cur" ]; then
        printf '%s%s%s %s%s%s%s\033[K\n' "$CLI_CYAN" "$_CLI_G_PTR" "$CLI_RESET" "$mark" "$CLI_BOLD" "$item" "$CLI_RESET"
      else
        printf '  %s%s\033[K\n' "$mark" "$item"
      fi >&2
      i=$((i + 1))
    done
    drawn=$page
    if [ "$n" -gt "$page" ]; then
      printf '  %s%d-%d of %d%s\033[K\n' "$CLI_DIM" "$top" "$last" "$n" "$CLI_RESET" >&2
      drawn=$((page + 1))
    fi
    _cli_key || _CLI_KEY=end-of-input
    case $_CLI_KEY in
      up|k|backtab) cur=$((cur - 1)); if [ "$cur" -lt 1 ]; then cur=$n; fi;;
      down|j|tab)   cur=$((cur + 1)); if [ "$cur" -gt "$n" ]; then cur=1; fi;;
      space)  if [ "$kind" = multi ]; then
                case $marks in *" $cur "*) marks=${marks/" $cur "/ };; *) marks="$marks$cur ";; esac
              fi;;
      a)      if [ "$kind" = multi ]; then
                if [ "$marks" = ' ' ]; then
                  i=1; while [ "$i" -le "$n" ]; do marks="$marks$i "; i=$((i + 1)); done
                else marks=' '; fi
              fi;;
      enter)  break;;
      esc|end-of-input)  # Escape is status 3, so that a wizard can step back; running out of input is 1
              _cli_raw_off
              printf '\033[%dA\033[J%s%s %s (cancelled)%s\n' $((drawn + 1)) "$CLI_DIM" "$_CLI_G_ERR" "$title" "$CLI_RESET" >&2
              if [ "$_CLI_KEY" = esc ]; then return 3; fi
              return 1;;
    esac
    printf '\033[%dA' "$drawn" >&2
  done
  _cli_raw_off

  if [ "$kind" = multi ]; then
    i=1
    while [ "$i" -le "$n" ]; do
      case $marks in *" $i "*) eval "item=\${$i}"; out="$out${out:+$_CLI_NL}$item"; shown="$shown${shown:+, }$item";; esac
      i=$((i + 1))
    done
  else
    eval "out=\${$cur}"; shown=$out
  fi
  printf '\033[%dA\033[J%s%s%s %s %s%s%s %s\n' $((drawn + 1)) "$CLI_GREEN" "$_CLI_G_OK" "$CLI_RESET" "$title" \
    "$CLI_DIM" "$_CLI_G_SEP" "$CLI_RESET" "${shown:-(none)}" >&2
  if [ -n "$out" ]; then printf '%s\n' "$out"; fi
}

_cli_pick_line() {  # the same question without a terminal: one line in, by number or by name
  local kind=$1 title=$2 defaults=$3
  shift 3
  local n=$# i=1 item='' ans='' part='' out='' opts='' got=0 hit=''
  for item in "$@"; do opts="$opts${opts:+ | }$item"; done
  if [ "$_CLI_MODE" != agent ]; then
    printf '%s\n' "$title" >&2
    for item in "$@"; do printf '  %d) %s\n' "$i" "$item" >&2; i=$((i + 1)); done
    printf 'choice%s: ' "${defaults:+ [${defaults//$_CLI_NL/, }]}" >&2
  fi
  if _cli_readline; then got=1; fi
  ans=$_CLI_R
  if [ "$_CLI_MODE" != agent ] && [ ! -t 0 ]; then printf '%s\n' "${ans:-${defaults//$_CLI_NL/, }}" >&2; fi

  if [ -z "$ans" ]; then
    out=$defaults
    if [ -z "$out" ] && { [ "$kind" = single ] || [ "$got" -eq 0 ]; }; then
      cli_error "input required: $title (options: $opts). Pipe the answer on stdin."
      return 1
    fi
  else
    if [ "$kind" = single ]; then ans="$ans,"; else ans="${ans%,},"; fi
    while [ -n "$ans" ]; do
      if [ "$kind" = single ]; then part=${ans%,}; ans=''; else part=${ans%%,*}; ans=${ans#*,}; fi
      _cli_trim "$part"; part=$_CLI_R; hit=''
      for item in "$@"; do if [ "$item" = "$part" ]; then hit=$item; break; fi; done
      if [ -z "$hit" ]; then
        case $part in
          ''|*[!0-9]*) ;;
          *) if [ "$part" -ge 1 ] && [ "$part" -le "$n" ]; then eval "hit=\${$part}"; fi;;
        esac
      fi
      if [ -z "$hit" ]; then
        cli_error "invalid choice '$part' for: $title (options: $opts)"
        return 1
      fi
      out="$out${out:+$_CLI_NL}$hit"
    done
  fi
  if [ "$_CLI_MODE" = agent ]; then printf '%s: %s\n' "$title" "${out//$_CLI_NL/, }" >&2; fi
  if [ -n "$out" ]; then printf '%s\n' "$out"; fi
}

cli_browse() {  # cli_browse [-a] [-d] QUESTION [DIR]   walk through the files under DIR → the chosen path on stdout
  # cli_browse -l [-d] [-s SEP] QUESTION ROW...   the same over a list of paths; ROW is "PATH|NOTE"
  # -d   choose a directory, not a file        -a   hidden files too (the "." key switches them)
  # keys: up/down move, right opens, left closes (on DIR itself: shows the directory above), enter chooses
  local sep='|' list=0 all=0 q='' dir='' out='' rc=0
  _CLI_F_DIRS=0
  while :; do
    case ${1:-} in
      -l) list=1; shift;;
      -a) all=1; shift;;
      -d) _CLI_F_DIRS=1; shift;;
      -s) sep=$2; shift 2;;
      *) break;;
    esac
  done
  q=${1:-}
  if [ $# -gt 0 ]; then shift; fi
  if [ "$list" -eq 1 ]; then
    if [ $# -gt 0 ]; then out=$(printf '%s\n' "$@" | _cli_f_awk "$sep"); fi
    set --
    _cli_f_build "$out"
  else
    dir=${1:-.}
    set --
    if [ ! -d "$dir" ]; then cli_error "$q: not a directory: $dir"; return 2; fi
    while :; do case $dir in ?*/) dir=${dir%/};; *) break;; esac; done
    _cli_f_root "$dir"; _CLI_F_DOT=$((1 - all))
  fi
  _cli_f_walk 0 0
  if [ "$_CLI_V_N" -eq 0 ]; then
    cli_error "$q: nothing to choose from"; rc=2
  elif _cli_tty; then
    _cli_browse_keys "$q" || rc=$?
  else
    _cli_browse_line "$q" "$dir" || rc=$?
  fi
  _cli_f_free
  return "$rc"
}

_cli_f_up() {  # put the directory above the top of the tree on top of it
  local old=$_CLI_F_TOP name='' above='' child='' new=0 i=0 prev=0 n=''
  eval "name=\$_cli_fn$old"
  case $name in
    /) return 0;;
    .) above=..;;
    ..|*/..) above=$name/..;;
    */*) above=${name%/*}; above=${above:-/};;
    *) above=.;;
  esac
  case $name in
    .|..|*/..) child=$(CDPATH='' cd -- "$name" 2>/dev/null && pwd -P) || child=''; child=${child##*/};;
    *) child=${name##*/};;
  esac
  if [ -z "$child" ] || [ ! -d "$above" ]; then return 0; fi
  new=$((_CLI_F_N + 1)); _CLI_F_N=$new
  eval "_cli_fn$new=\$above _cli_fp$new=0 _cli_fc$new=0 _cli_fs$new=0 _cli_fk$new=d _cli_fo$new=1 _cli_fl$new=0 _cli_ft$new=''"
  _CLI_F_TOP=$new
  _cli_f_load "$new"
  eval "i=\$_cli_fc$new"
  while [ "$i" -gt 0 ]; do  # the old top takes the place of its own entry
    eval "n=\$_cli_fn$i"
    if [ "$n" = "$child" ]; then
      eval "n=\$_cli_fs$i"
      eval "_cli_fs$old=$n _cli_fp$old=$new _cli_fn$old=\$child"
      if [ "$prev" -eq 0 ]; then eval "_cli_fc$new=$old"; else eval "_cli_fs$prev=$old"; fi
      return 0
    fi
    prev=$i; eval "i=\$_cli_fs$i"
  done
}

_cli_browse_keys() {  # QUESTION
  local title=$1 page=${CLI_PAGE:-10} n=$_CLI_V_N cur=1 curid=0 top=1 dirty=0 drawn=0 r=0 id=0 last=0 maxw=0 avail=0
  local k='' o=0 name='' note='' draw='' style='' hint='' out='' key='' i=0
  case $page in ''|*[!0-9]*|0) page=10;; esac
  _cli_width; maxw=$((_CLI_R - 3))
  if [ "$_CLI_G_KEYS" = 'up/down' ]; then hint='up/down move, right open, left close, enter select'
  else hint="$_CLI_G_KEYS move, → open, ← close, enter select"; fi
  if [ "$_CLI_F_FS" = 1 ]; then hint="$hint, . hidden files"; fi
  printf '%s?%s %s%s%s  %s%s%s\n' "$CLI_CYAN" "$CLI_RESET" "$CLI_BOLD" "$title" "$CLI_RESET" "$CLI_DIM" "$hint" "$CLI_RESET" >&2

  _cli_raw_on
  while :; do
    if [ "$dirty" -eq 1 ]; then  # the tree changed: which rows are on show, and where is the cursor now
      _cli_f_walk 0 0; n=$_CLI_V_N; dirty=0
      while :; do
        cur=0; r=1
        while [ "$r" -le "$n" ]; do
          eval "id=\$_cli_vi$r"
          if [ "$id" -eq "$curid" ]; then cur=$r; break; fi
          r=$((r + 1))
        done
        if [ "$cur" -gt 0 ]; then break; fi
        eval "curid=\$_cli_fp$curid"  # its row is gone: the directory it was in
        if [ "$curid" -eq 0 ]; then cur=1; break; fi
      done
    fi
    if [ "$cur" -lt "$top" ]; then top=$cur; fi
    if [ "$cur" -ge $((top + page)) ]; then top=$((cur - page + 1)); fi
    if [ "$top" -gt $((n - page + 1)) ]; then top=$((n - page + 1)); fi
    if [ "$top" -lt 1 ]; then top=1; fi
    last=$((top + page - 1)); if [ "$last" -gt "$n" ]; then last=$n; fi
    r=$top
    while [ "$r" -le "$last" ]; do
      eval "id=\$_cli_vi$r draw=\$_cli_vx$r"
      eval "name=\$_cli_fn$id k=\$_cli_fk$id note=\$_cli_ft$id"
      _cli_f_name "$name" "$k"; name=$_CLI_R
      avail=$((maxw - ${#draw})); if [ "$avail" -lt 1 ]; then avail=1; fi
      name=${name:0:$avail}
      if [ -n "$note" ] && [ $((${#name} + 2 + ${#note})) -le "$avail" ]; then note="  $CLI_DIM$note$CLI_RESET"; else note=''; fi
      style=''
      if [ "$r" -eq "$cur" ]; then style=$CLI_BOLD; fi
      if [ "$k" != f ]; then style="$style$CLI_BLUE"; fi
      if [ "$r" -eq "$cur" ]; then printf '%s%s%s ' "$CLI_CYAN" "$_CLI_G_PTR" "$CLI_RESET"; else printf '  '; fi >&2
      printf '%s%s%s%s%s\033[K\n' "${draw:+$CLI_DIM$draw$CLI_RESET}" "$style" "$name" "${style:+$CLI_RESET}" "$note" >&2
      r=$((r + 1))
    done
    drawn=$((last - top + 1))
    if [ "$n" -gt "$page" ]; then
      printf '  %s%d-%d of %d%s\033[K\n' "$CLI_DIM" "$top" "$last" "$n" "$CLI_RESET" >&2
      drawn=$((drawn + 1))
    fi
    printf '\033[J' >&2

    _cli_key || _CLI_KEY=end-of-input
    key=$_CLI_KEY
    eval "curid=\$_cli_vi$cur"
    eval "k=\$_cli_fk$curid o=\$_cli_fo$curid i=\$_cli_fp$curid"
    if [ "$_CLI_F_FS" = 1 ] && [ "$i" -eq 0 ]; then k=top; fi  # DIR itself: always open
    case $key in
      up|k|backtab) cur=$((cur - 1)); if [ "$cur" -lt 1 ]; then cur=$n; fi;;
      down|j|tab)   cur=$((cur + 1)); if [ "$cur" -gt "$n" ]; then cur=1; fi;;
      home)   cur=1;;
      end)    cur=$n;;
      right|l)
              case $k in
                f) ;;
                *) if [ "$o" = 1 ]; then  # open already: step inside
                     if [ "$cur" -lt "$n" ]; then
                       eval "id=\$_cli_vi$((cur + 1))"; eval "id=\$_cli_fp$id"
                       if [ "$id" -eq "$curid" ]; then cur=$((cur + 1)); fi
                     fi
                   else eval "_cli_fo$curid=1"; dirty=1; fi;;
              esac;;
      left|h)
              if [ "$k" = top ]; then _cli_f_up; dirty=1
              elif [ "$k" != f ] && [ "$o" = 1 ]; then eval "_cli_fo$curid=0"; dirty=1
              elif [ "$i" -gt 0 ]; then curid=$i; dirty=1; fi;;
      space)  case $k in f|top) ;; *) eval "_cli_fo$curid=$((1 - o))"; dirty=1;; esac;;
      .)      if [ "$_CLI_F_FS" = 1 ]; then _CLI_F_DOT=$((1 - _CLI_F_DOT)); dirty=1; fi;;
      enter)
              if [ "$k" = f ] || [ "$_CLI_F_DIRS" = 1 ]; then break; fi
              if [ "$k" != top ]; then eval "_cli_fo$curid=$((1 - o))"; dirty=1; fi;;
      esc|end-of-input)  # Escape is status 3, so that a wizard can step back; running out of input is 1
              _cli_raw_off
              printf '\033[%dA\033[J%s%s %s (cancelled)%s\n' $((drawn + 1)) "$CLI_DIM" "$_CLI_G_ERR" "$title" "$CLI_RESET" >&2
              if [ "$key" = esc ]; then return 3; fi
              return 1;;
    esac
    printf '\033[%dA' "$drawn" >&2
  done
  _cli_raw_off

  _cli_f_path "$curid"; out=$_CLI_R
  if [ "$_CLI_F_FS" = 1 ]; then case $out in ./?*) out=${out#./};; esac; fi
  printf '\033[%dA\033[J%s%s%s %s %s%s%s %s\n' $((drawn + 1)) "$CLI_GREEN" "$_CLI_G_OK" "$CLI_RESET" "$title" \
    "$CLI_DIM" "$_CLI_G_SEP" "$CLI_RESET" "$out" >&2
  printf '%s\n' "$out"
}

_cli_browse_line() {  # QUESTION DIR   the same question without a terminal: one line in, a path
  local q=$1 dir=$2 what='a file' ans='' hit='' opts='' i=0 k='' try=''
  if [ "$_CLI_F_DIRS" = 1 ]; then what='a directory'; fi
  if [ "$_CLI_F_FS" = 1 ]; then what="$what under $dir"; fi
  if [ "$_CLI_MODE" != agent ]; then printf '%s (%s): ' "$q" "$what" >&2; fi
  _cli_readline || true
  ans=$_CLI_R
  if [ "$_CLI_MODE" != agent ] && [ ! -t 0 ]; then printf '%s\n' "$ans" >&2; fi
  if [ "$_CLI_F_FS" = 1 ]; then
    for try in "$ans" "$dir/$ans"; do  # as typed, or under DIR
      if [ -z "$ans" ] || [ -n "$hit" ]; then break; fi
      if [ "$_CLI_F_DIRS" = 1 ]; then
        if [ -d "$try" ]; then hit=$try; fi
      elif [ -e "$try" ] && [ ! -d "$try" ]; then hit=$try; fi
    done
    case $hit in ./?*) hit=${hit#./};; esac
  else
    while [ "$i" -lt "$_CLI_F_N" ]; do
      i=$((i + 1)); eval "k=\$_cli_fk$i"
      if [ "$_CLI_F_DIRS" = 1 ]; then
        if [ "$k" = f ]; then continue; fi
      elif [ "$k" != f ]; then continue; fi
      _cli_f_path "$i"
      opts="$opts${opts:+ | }$_CLI_R"
      if [ "$_CLI_R" = "$ans" ] || [ "$_CLI_R/" = "$ans" ]; then hit=$_CLI_R; fi
    done
    what="$what: $opts"
  fi
  if [ -z "$ans" ]; then cli_error "input required: $q ($what). Pipe the path on stdin."; return 1; fi
  if [ -z "$hit" ]; then cli_error "invalid choice '$ans' for: $q ($what)"; return 1; fi
  if [ "$_CLI_MODE" = agent ]; then printf '%s: %s\n' "$q" "$hit" >&2; fi
  printf '%s\n' "$hit"
}

# ── commands, options and help pages ─────────────────────────────────────────
#
#   cli_app todo "A tiny todo list" 1.0.0
#   cli_opt "-v, --verbose" "Say more"                # before any cli_cmd: a global option
#   cli_cmd add "Add a task"                          # runs cmd_add (or name a function as 3rd argument)
#   cli_args "<text>"
#   cli_opt "-p, --priority <n>" "1 (high) to 3" 2    # sets $opt_priority, default 2
#   cli_example 'todo add "Buy milk" -p 1'
#   cli_run "$@"
#
# A command that declares options gets them parsed into $opt_<name> (flags: 1 or empty) and
# receives only the positional arguments. Without any declared option it gets "$@" untouched.

cli_app() {  # cli_app NAME [DESCRIPTION] [VERSION]
  _CLI_APP=$1 _CLI_APP_DESC=${2:-} _CLI_APP_VER=${3:-}
}

cli_cmd() {  # cli_cmd NAME DESCRIPTION [FUNCTION]   FUNCTION defaults to cmd_NAME
  local name=$1 fn=${3:-}
  if [ -z "$fn" ]; then fn="cmd_${name//-/_}"; fi
  _CLI_CMDS="$_CLI_CMDS$name$_CLI_US$fn$_CLI_US${2:-}$_CLI_NL"
  _CLI_CUR=$name
}

cli_args() {  # cli_args SYNOPSIS   e.g. "<file> [more...]", for the command declared last
  _CLI_SYN="$_CLI_SYN$_CLI_CUR$_CLI_US$1$_CLI_NL"
}

cli_example() {  # cli_example TEXT   an example line for the command declared last
  _CLI_EX="$_CLI_EX$_CLI_CUR$_CLI_US$1$_CLI_NL"
}

cli_opt() {  # cli_opt "-o, --out <dir>" DESCRIPTION [DEFAULT]   for the command declared last
  local flags=$1 rest tok short=- long=- takes=0 var
  rest="${flags//,/ } "
  while [ -n "$rest" ]; do
    tok=${rest%% *}; rest=${rest#* }
    case $tok in '') ;; --*) long=$tok;; -*) short=$tok;; *) takes=1;; esac
  done
  if [ "$long" != - ]; then var=${long#--}; else var=${short#-}; fi
  var=${var//[^A-Za-z0-9]/_}
  eval "opt_$var=\${3:-}"
  _CLI_OPTS="$_CLI_OPTS$_CLI_CUR$_CLI_US$var$_CLI_US$takes$_CLI_US$short$_CLI_US$long$_CLI_US$flags$_CLI_US${3:--}$_CLI_US${2:-}$_CLI_NL"
}

_cli_find_cmd() {  # NAME → $_CLI_FN
  local name fn desc
  if [ -z "$1" ]; then return 1; fi
  while IFS=$_CLI_US read -r name fn desc; do
    if [ "$name" = "$1" ]; then _CLI_FN=$fn; _CLI_R=$desc; return 0; fi
  done <<<"$_CLI_CMDS"
  return 1
}

_cli_find_opt() {  # COMMAND FLAG → $_CLI_OV (variable) $_CLI_OT (takes a value)
  local o var takes short long flags def desc
  while IFS=$_CLI_US read -r o var takes short long flags def desc; do
    case $o in "$1"|@app)
      if [ "$2" = "$short" ] || [ "$2" = "$long" ]; then _CLI_OV=$var; _CLI_OT=$takes; return 0; fi;;
    esac
  done <<<"$_CLI_OPTS"
  return 1
}

_cli_last() {  # STORE OWNER → the value recorded for OWNER
  local o v
  _CLI_R=''
  while IFS=$_CLI_US read -r o v; do
    if [ "$o" = "$2" ]; then _CLI_R=$v; fi
  done <<<"$1"
}

_cli_opt_rows() {  # OWNER → "flags<US>description" lines
  local o var takes short long flags def desc
  _CLI_R=''
  while IFS=$_CLI_US read -r o var takes short long flags def desc; do
    if [ "$o" != "$1" ]; then continue; fi
    if [ "$def" != - ]; then desc="$desc (default: $def)"; fi
    _CLI_R="$_CLI_R$flags$_CLI_US$desc$_CLI_NL"
  done <<<"$_CLI_OPTS"
}

_cli_cols() {  # ROWS INDENT   print "left<US>right" lines as two columns (unaligned for agents)
  local l r w=0
  while IFS=$_CLI_US read -r l r; do
    if [ "${#l}" -gt "$w" ]; then w=${#l}; fi
  done <<<"$1"
  while IFS=$_CLI_US read -r l r; do
    if [ -z "$l" ]; then continue; fi
    if [ "$_CLI_MODE" = agent ]; then printf '%s%s  %s\n' "$2" "$l" "$r"; continue; fi
    _cli_pad "$l" "$w"
    printf '%s%s%s%s  %s\n' "$2" "$CLI_CYAN" "$_CLI_R" "$CLI_RESET" "$r"
  done <<<"$1"
}

_cli_h() {  # a section heading in a help page
  printf '\n%s%s%s\n' "$CLI_BOLD" "$1" "$CLI_RESET"
}

cli_help() {  # cli_help [COMMAND]   the help page for the app, or for one command
  local cmd=${1:-} name fn desc rows='' syn='' ex='' o v
  if [ -n "$cmd" ]; then
    _cli_find_cmd "$cmd" || { _cli_unknown "$cmd"; return 2; }
    desc=$_CLI_R
    _cli_last "$_CLI_SYN" "$cmd"; syn=$_CLI_R
    _cli_opt_rows "$cmd"; rows=$_CLI_R
    _cli_opt_rows @app; rows="$rows$_CLI_R"
    if [ "$_CLI_MODE" = agent ]; then
      printf '%s %s%s  %s\n' "$_CLI_APP" "$cmd" "${syn:+ $syn}" "$desc"
      _cli_cols "$rows" '  '
      while IFS=$_CLI_US read -r o v; do
        if [ "$o" = "$cmd" ]; then printf '  e.g. %s\n' "$v"; fi
      done <<<"$_CLI_EX"
      return 0
    fi
    printf '%s%s %s%s%s\n' "$CLI_BOLD" "$_CLI_APP" "$cmd" "$CLI_RESET" "${desc:+ - $desc}"
    _cli_h USAGE
    printf '  %s %s%s%s\n' "$_CLI_APP" "$cmd" "${rows:+ [options]}" "${syn:+ $syn}"
    if [ -n "$rows" ]; then _cli_h OPTIONS; _cli_cols "$rows" '  '; fi
    while IFS=$_CLI_US read -r o v; do
      if [ "$o" != "$cmd" ]; then continue; fi
      if [ -z "$ex" ]; then _cli_h EXAMPLES; ex=1; fi
      printf '  %s\n' "$v"
    done <<<"$_CLI_EX"
    return 0
  fi

  if [ "$_CLI_MODE" = agent ]; then  # the whole reference in one call
    printf '%s%s%s\n' "$_CLI_APP" "${_CLI_APP_VER:+ $_CLI_APP_VER}" "${_CLI_APP_DESC:+ - $_CLI_APP_DESC}"
    printf 'usage: %s <command> [args]\ncommands:\n' "$_CLI_APP"
    while IFS=$_CLI_US read -r name fn desc; do
      if [ -z "$name" ]; then continue; fi
      _cli_last "$_CLI_SYN" "$name"
      printf '  %s%s  %s\n' "$name" "${_CLI_R:+ $_CLI_R}" "$desc"
      _cli_opt_rows "$name"; _cli_cols "$_CLI_R" '    '
    done <<<"$_CLI_CMDS"
    _cli_opt_rows @app
    if [ -n "$_CLI_R" ]; then printf 'options:\n'; _cli_cols "$_CLI_R" '  '; fi
    return 0
  fi

  printf '%s%s%s%s%s\n' "$CLI_BOLD" "$_CLI_APP" "$CLI_RESET" "${_CLI_APP_VER:+ $_CLI_APP_VER}" "${_CLI_APP_DESC:+ - $_CLI_APP_DESC}"
  _cli_h USAGE
  printf '  %s <command> [options]\n' "$_CLI_APP"
  while IFS=$_CLI_US read -r name fn desc; do
    if [ -n "$name" ]; then rows="$rows$name$_CLI_US$desc$_CLI_NL"; fi
  done <<<"$_CLI_CMDS"
  _cli_h COMMANDS
  _cli_cols "$rows" '  '
  _cli_opt_rows @app; rows=$_CLI_R
  rows="$rows-h, --help${_CLI_US}Show help (also: $_CLI_APP help <command>)$_CLI_NL"
  if [ -n "$_CLI_APP_VER" ]; then rows="$rows-V, --version${_CLI_US}Show the version$_CLI_NL"; fi
  rows="$rows-i, --interactive${_CLI_US}Start the interactive prompt$_CLI_NL"
  rows="$rows--agent, --plain${_CLI_US}Terse output for LLM agents / no colours$_CLI_NL"
  _cli_h OPTIONS
  _cli_cols "$rows" '  '
}

_cli_unknown() {  # COMMAND
  local name fn desc guess='' all=''
  while IFS=$_CLI_US read -r name fn desc; do
    if [ -z "$name" ]; then continue; fi
    all="$all${all:+ }$name"
    case $name in "$1"*) guess=$name;; "${1:0:1}"*) if [ -z "$guess" ]; then guess=$name; fi;; esac
  done <<<"$_CLI_CMDS"
  if [ "$_CLI_MODE" = agent ]; then cli_error "unknown command '$1'; commands: $all"; return 0; fi
  cli_error "unknown command '$1'${guess:+. Did you mean '$guess'?}"
  cli_hint "Run '$_CLI_APP help' to see every command."
}

_cli_dispatch() {  # COMMAND [ARGS...]
  local cmd=$1 run strict=0 arg val o var takes short long flags def desc
  local -a pos
  shift
  if [ "$cmd" = help ]; then cli_help "$@"; return; fi
  _cli_find_cmd "$cmd" || { _cli_unknown "$cmd"; return 2; }
  run=$_CLI_FN
  while IFS=$_CLI_US read -r o var takes short long flags def desc; do
    case $o in "$cmd"|@app)
      strict=1
      if [ "$o" = "$cmd" ]; then if [ "$def" = - ]; then def=''; fi; eval "opt_$var=\$def"; fi;;
    esac
  done <<<"$_CLI_OPTS"
  if [ "$strict" -eq 0 ]; then
    case ${1:-} in -h|--help) cli_help "$cmd"; return 0;; esac
    "$run" "$@"
    return
  fi
  pos=()
  while [ $# -gt 0 ]; do
    arg=$1; shift
    case $arg in
      --) while [ $# -gt 0 ]; do pos+=("$1"); shift; done;;
      -h|--help) cli_help "$cmd"; return 0;;
      --*=*)
        val=${arg#*=}; arg=${arg%%=*}
        if ! _cli_find_opt "$cmd" "$arg" || [ "$_CLI_OT" != 1 ]; then
          cli_error "unknown option '$arg=' for '$cmd'"; return 2
        fi
        eval "opt_$_CLI_OV=\$val";;
      -?*)
        if ! _cli_find_opt "$cmd" "$arg"; then
          cli_error "unknown option '$arg' for '$cmd'"
          cli_hint "Run '$_CLI_APP help $cmd' to see its options."
          return 2
        fi
        if [ "$_CLI_OT" != 1 ]; then eval "opt_$_CLI_OV=1"; continue; fi
        if [ $# -eq 0 ]; then cli_error "option '$arg' needs a value"; return 2; fi
        eval "opt_$_CLI_OV=\$1"; shift;;
      *) pos+=("$arg");;
    esac
  done
  "$run" ${pos[@]+"${pos[@]}"}
}

cli_run() {  # cli_run "$@"   parse the command line and run the matching command
  local -a pre
  pre=()
  while [ $# -gt 0 ]; do
    case $1 in
      --agent) cli_mode agent;;
      --plain) cli_mode plain;;
      --no-color) NO_COLOR=1; export NO_COLOR; _cli_setup;;
      -h|--help) cli_help; return 0;;
      -V|--version) printf '%s%s\n' "$_CLI_APP" "${_CLI_APP_VER:+ $_CLI_APP_VER}"; return 0;;
      -i|--interactive) cli_interactive; return;;
      --*=*) pre+=("$1");;
      -?*)
        if ! _cli_find_opt @app "$1"; then cli_error "unknown option '$1'"; return 2; fi
        pre+=("$1")
        if [ "$_CLI_OT" = 1 ] && [ $# -gt 1 ]; then pre+=("$2"); shift; fi;;
      *) break;;
    esac
    shift
  done
  if [ $# -eq 0 ]; then cli_help; return 0; fi
  _cli_dispatch "$1" ${pre[@]+"${pre[@]}"} "${@:2}"
}

_cli_ps() { _CLI_R="$CLI_CYAN$_CLI_APP$CLI_RESET> "; }  # the prompt of cli_interactive

cli_interactive() {  # cli_interactive   a prompt that runs the app's commands until "exit"
  # Tab and Shift-Tab step through the commands, and after a command through its options.
  local line='' words='' w='' rc=0 ps='' name fn desc width=0
  _cli_ps; ps=$_CLI_R
  if _cli_tty; then
    cli_hint "$_CLI_APP interactive: type a command, 'help' or 'exit'. Tab completes, an empty line opens a menu."
  fi
  while :; do
    if _cli_tty; then
      _cli_edit "$ps" line || break
      line=$_CLI_R
      case $line in *[![:space:]]*) _CLI_HIST="$_CLI_HIST$line$_CLI_NL"; _CLI_HIST_N=$((_CLI_HIST_N + 1));; esac
    else
      IFS= read -r line || [ -n "$line" ] || break
    fi
    case $line in
      exit|quit|q) break;;
      '?') cli_help; continue;;
      *[![:space:]]*) ;;
      *) # empty line: pick a command from a menu
        _cli_tty || continue
        set --
        while IFS=$_CLI_US read -r name fn desc; do
          if [ "${#name}" -gt "$width" ]; then width=${#name}; fi
        done <<<"$_CLI_CMDS"
        while IFS=$_CLI_US read -r name fn desc; do
          if [ -n "$name" ]; then _cli_pad "$name" "$width"; set -- "$@" "$_CLI_R  $desc"; fi
        done <<<"$_CLI_CMDS"
        line=$(_cli_pick single "Run a command" '' "$@") || continue
        line=${line%% *};;
    esac
    words=$(printf '%s\n' "$line" | xargs printf '%s\n' 2>/dev/null) || { cli_error "cannot parse: $line"; continue; }
    set --
    while IFS= read -r w; do set -- "$@" "$w"; done <<<"$words"
    rc=0
    ( _cli_dispatch "$@" ) || rc=$?
    if [ "$rc" -ne 0 ]; then cli_hint "(exit $rc)"; fi
  done
  return 0
}

# ── odds and ends ────────────────────────────────────────────────────────────

cli_has() {  # cli_has COMMAND   true when COMMAND is installed
  command -v "$1" >/dev/null 2>&1
}

cli_require() {  # cli_require COMMAND...   exit 127 unless every COMMAND is installed
  local c missing=''
  for c in "$@"; do cli_has "$c" || missing="$missing $c"; done
  if [ -n "$missing" ]; then cli_die "missing required command(s):$missing" 127; fi
}

cli_legacy_aliases() {  # cli_legacy_aliases   define the names of the old sh-utils utils.script
  success() { cli_ok "$@"; }
  error() { cli_error "$@"; }
  warning() { cli_warn "$@"; }
  info() { cli_info "$@"; }
  message() { printf '%s\n' "$*"; }
  log_success() { cli_ok "$@"; }
  log_error() { cli_error "$@"; }
  log_warning() { cli_warn "$@"; }
  log_info() { cli_info "$@"; }
  command_exists() { cli_has "$1"; }
  to_upper() { printf '%s\n' "$1" | tr '[:lower:]' '[:upper:]'; }
  to_lower() { printf '%s\n' "$1" | tr '[:upper:]' '[:lower:]'; }
  trim() { _cli_trim "$1"; printf '%s\n' "$_CLI_R"; }
  pause() { sleep "$1"; }
}

_cli_setup

# ── run directly: ./cli.sh FUNCTION [ARGS...]  (the cli_ prefix is optional) ──

if [ -n "${ZSH_VERSION:-}" ]; then
  case ${ZSH_EVAL_CONTEXT:-} in *:file*) _CLI_MAIN=0;; *) _CLI_MAIN=1;; esac
elif [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then _CLI_MAIN=1
else _CLI_MAIN=0; fi

if [ "$_CLI_MAIN" = 1 ]; then
  if [ $# -eq 0 ]; then
    printf 'cli.sh %s - source it (. ./cli.sh), or run one function: cli.sh FUNCTION [ARGS...]\n\n' "$CLI_VERSION"
    sed -n 's/^cli_[a-z_]*() {  # \(cli_.*\)/  \1/p' "$0"
    exit 0
  fi
  _CLI_FN="cli_${1#cli_}"; _CLI_FN=${_CLI_FN//-/_}; shift
  if ! typeset -f "$_CLI_FN" >/dev/null 2>&1; then cli_error "no such function: $_CLI_FN"; exit 2; fi
  "$_CLI_FN" "$@"
fi
