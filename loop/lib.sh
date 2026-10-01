#!/usr/bin/env bash
# Shared helpers for the temporary development loop (removed before hand-off).
set -u
OUT="${RUNNER_TEMP:-/tmp}/loop-out"
mkdir -p "$OUT"
PY="$(command -v python3 || command -v python)"
export LOOP_OUT="$OUT"

# run NAME CMD...  -> $OUT/NAME.log with trailing [exit=..] marker; never aborts the script.
run() {
  local name="$1"; shift
  local started code
  started=$(date +%s)
  echo "::group::$name"
  "$@" >"$OUT/$name.log" 2>&1
  code=$?
  echo "[exit=$code elapsed=$(( $(date +%s) - started ))s]" >>"$OUT/$name.log"
  tail -n 5 "$OUT/$name.log"
  echo "::endgroup::"
  echo "== $name exit=$code"
  return 0
}

# emit TITLE FILE [emit.py args...]
emit() {
  "$PY" "$GITHUB_WORKSPACE/loop/emit.py" "$@"
}
