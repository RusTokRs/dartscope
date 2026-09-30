#!/usr/bin/env bash
# Remaining runtime cases: method-form matrix (A3), package_config/uri-graph (C09/C10), per-file timing in flutter/samples.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
run apply_unblock git apply --verbose docs/development/audit-2026-09-30-unblock.patch
run build_cli cargo build --release -p dartscope-cli --locked --message-format short
emit build_cli "$OUT/build_cli.log" --chunk 1200 --max 1 --tail
if [ ! -x "target/release/dartscope" ]; then echo "CLI did not build"; exit 0; fi
run rt_a3 "$PY" audit-probe/runtime.py a3
emit a3 "$OUT/rt_a3.log" --chunk 3900 --max 3
run rt_c9 "$PY" audit-probe/runtime.py c9
emit c9 "$OUT/rt_c9.log" --chunk 3900 --max 3
SAMPLES="$RUNNER_TEMP/samples"
git clone --depth 1 --quiet https://github.com/flutter/samples.git "$SAMPLES" 2>>"$OUT/clone.log" || echo "clone failed"
export SAMPLES_DIR="$SAMPLES"
run rt_heavy perl -e 'alarm shift; exec @ARGV' 1500 "$PY" audit-probe/runtime.py heavy
emit heavy "$OUT/rt_heavy.log" --chunk 3900 --max 3
