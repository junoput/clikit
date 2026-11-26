#!/usr/bin/env bash
# Run the shared conformance cases (../../conformance/cases.txt) against cli.sh.
#   bash sh/test/conform.sh   |   zsh sh/test/conform.sh
set -u

HERE=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
LIB="$HERE/../cli.sh"
CASES="$HERE/../../conformance/cases.txt"
if [ -n "${ZSH_VERSION:-}" ]; then SH=zsh; else SH=${BASH:-bash}; fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/cli-conform.XXXXXX")
NL=$'\n' ESC=$'\033' TAB=$'\t' CR=$'\r' DEL=$'\177'
PASS=0 FAIL=0

decode() {  # TEXT → $R, with \e \t \r \s \b replaced; a trailing \c means "no newline"
  R=$1
  R=${R//\\e/$ESC}; R=${R//\\t/$TAB}; R=${R//\\r/$CR}; R=${R//\\s/ }; R=${R//\\b/$DEL}
  case $R in *\\c) R=${R%\\c};; *) R="$R$NL";; esac
}

reset() { name='' mode='' envs='' cmds='' input='' has_input=0 want_out='' want_err='' want_rc=0; }

finish() {  # run the case collected so far
  local rc=0 timed=0
  if [ -z "$name" ]; then return 0; fi
  printf '%s. "%s"\n%s' "$envs" "$LIB" "$cmds" >"$TMP/script"
  printf '%s' "$input" >"$TMP/in"
  if [ "$has_input" -eq 0 ]; then : >"$TMP/in"; fi
  env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" CLI_MODE="$mode" COLUMNS=80 LANG=en_US.UTF-8 TERM=xterm \
    "$SH" "$TMP/script" <"$TMP/in" >"$TMP/out" 2>"$TMP/err" || rc=$?
  case "$want_out$want_err" in *'<T>'*) timed=1;; esac
  if [ "$timed" -eq 1 ]; then
    sed -E 's/[0-9]+(\.[0-9])?m?s/<T>/g' "$TMP/out" >"$TMP/out.n" && mv "$TMP/out.n" "$TMP/out"
    sed -E 's/[0-9]+(\.[0-9])?m?s/<T>/g' "$TMP/err" >"$TMP/err.n" && mv "$TMP/err.n" "$TMP/err"
  fi
  printf '%s' "$want_out" >"$TMP/want_out"
  printf '%s' "$want_err" >"$TMP/want_err"
  if cmp -s "$TMP/out" "$TMP/want_out" && cmp -s "$TMP/err" "$TMP/want_err" && [ "$rc" -eq "$want_rc" ]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    printf 'FAIL [%s] %s (status %s, expected %s)\n' "$mode" "$name" "$rc" "$want_rc"
    cmp -s "$TMP/out" "$TMP/want_out" || { echo '  stdout expected:'; cat -v "$TMP/want_out"; echo '  stdout actual:'; cat -v "$TMP/out"; }
    cmp -s "$TMP/err" "$TMP/want_err" || { echo '  stderr expected:'; cat -v "$TMP/want_err"; echo '  stderr actual:'; cat -v "$TMP/err"; }
  fi
  reset
}

reset
cd "$HERE/../../conformance" || exit 1  # the tree and browse cases read conformance/files
while IFS= read -r line || [ -n "$line" ]; do
  case $line in
    '#'*) ;;
    '') finish;;
    '['*) finish; mode=${line#\[}; mode=${mode%%\]*}; name=${line#*\] };;
    '% '*) line=${line#% }; envs="${envs}export ${line%%=*}='${line#*=}'$NL";;
    '$ '*) cmds="${cmds}cli_${line#\$ }$NL";;
    '<'*) line=${line#<}; decode "${line# }"; input="$input$R"; has_input=1;;
    '|'*) decode "${line#|}"; want_out="$want_out$R";;
    '!'*) decode "${line#!}"; want_err="$want_err$R";;
    '= '*) want_rc=${line#= };;
  esac
done <"$CASES"
finish
rm -rf "$TMP"

printf '%s: %d conformance cases passed, %d failed\n' "$SH ${BASH_VERSION:-${ZSH_VERSION:-}}" "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
