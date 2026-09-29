#!/usr/bin/env bash
# Contract check: the webview and the Rust core stay wired end to end.
# Read-only. Exit 1 on any mismatch. Run from anywhere inside the repo.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
fail=0
same() { # label, left, right
  if [ "$2" = "$3" ]; then echo "ok   $1 ($(echo "$2" | grep -c .))"
  else echo "FAIL $1"; diff <(echo "$2") <(echo "$3") | sed -n 's/^[<>]/    &/p'; fail=1; fi
}

# 1. Every command api.js invokes is registered in generate_handler!, and vice versa.
#    Takes the last path segment, so it keeps working after the commands/ split.
js=$(grep -rhoE "invoke\('[a-z_]+'" ui/js | sed -E "s/invoke\('//; s/'//" | sort -u)
rs=$(sed -n '/generate_handler!\[/,/\])/p' src-tauri/src/main.rs | tail -n +2 \
     | grep -oE '[a-z_]+(::[a-z_]+)+' | sed -E 's/.*:://' | sort -u)
same "IPC commands  api.js == generate_handler!" "$js" "$rs"

# 2. Every app event Rust emits is listened for in the UI, and vice versa.
rs_ev=$(grep -rhoE 'emit\("[a-z-]+"' src-tauri/src | sed -E 's/emit\("//; s/"//' | sort -u)
js_ev=$(grep -rhoE "onEvent\('[a-z-]+'" ui/js | sed -E "s/onEvent\('//; s/'//" | sort -u)
same "events        emit(...) == onEvent(...)" "$rs_ev" "$js_ev"

# 3. Every static data-act / data-input / data-form value has a handler key somewhere in ui/js.
for attr in act input form; do
  missing=""
  for k in $(grep -rhoE "data-$attr=\"[a-z-]+\"" ui/js | sed -E "s/data-$attr=\"//; s/\"//" | sort -u); do
    grep -rqE "(^|[{,[:space:]])'?$k'?[[:space:]]*:" ui/js || missing="$missing $k"
  done
  [ -z "$missing" ] && echo "ok   data-$attr keys all handled" || { echo "FAIL data-$attr unhandled:$missing"; fail=1; }
done
exit $fail
