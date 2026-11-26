#!/bin/sh
# Run the unit tests and the shared conformance cases under every supported shell found here.
here=$(cd "$(dirname "$0")" && pwd)
failed=0
seen=''
for shell in /bin/bash "$(command -v bash)" "$(command -v zsh)"; do
  [ -x "$shell" ] || continue
  case " $seen " in *" $shell "*) continue;; esac
  seen="$seen $shell"
  "$shell" "$here/test.sh" || failed=1
  "$shell" "$here/conform.sh" || failed=1
done
exit "$failed"
