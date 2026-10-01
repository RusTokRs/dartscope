# TEMPORARY perf measurements (removed before hand-off): Linux only, for commits tagged [perf].
msg="$("$PY" -c "import json,os;print(json.load(open(os.environ['GITHUB_EVENT_PATH'])).get('head_commit',{}).get('message',''))" 2>/dev/null)"
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[perf\]'; then
  run perf_build cargo build --release -p dartscope-cli --locked --message-format short
  if [ -x "$GITHUB_WORKSPACE/target/release/dartscope" ]; then
    run perf_clone git clone --depth 1 -q https://github.com/flutter/samples "$RUNNER_TEMP/samples"
    PERF_CORPUS="$RUNNER_TEMP/samples" run perf_cli "$PY" loop/perf.py
    emit perf_cli "$OUT/perf_cli.log" --chunk 3900 --max 3
  else
    emit perf_build "$OUT/perf_build.log" --chunk 3000 --max 1 --tail
  fi
  run perf_index cargo test --release -p dartscope-index --test zz_loop_perf --locked -- --ignored --nocapture --test-threads=1
  grep -E '^(files=|test result|error)' "$OUT/perf_index.log" > "$OUT/perf_index.sum"
  emit perf_index "$OUT/perf_index.sum" --chunk 3900 --max 2
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[perf\]'; then
  run perf_phase cargo test --release -p dartscope-parse --lib zz_ --locked -- --ignored --nocapture --test-threads=1
  grep -E '^(phase|test result|error)' "$OUT/perf_phase.log" > "$OUT/perf_phase.sum"
  emit perf_phase "$OUT/perf_phase.sum" --chunk 3900 --max 2
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[hunt\]'; then
  : >"$OUT/hunt.sum"
  for salt in 1 2 3 4 5 6; do
    DARTSCOPE_MUTATION_SEED=$salt DARTSCOPE_MUTATION_ROUNDS=6000 run "hunt_$salt" cargo test --release -p dartscope-parse --test robustness_mutations --locked -- --nocapture
    { echo "== seed $salt =="; grep -E '^(panic at|span problem|test result|error)' "$OUT/hunt_$salt.log" | cut -c1-300 | head -14; } >>"$OUT/hunt.sum"
  done
  emit hunt "$OUT/hunt.sum" --chunk 3900 --max 2
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[hunti\]'; then
  : >"$OUT/hunti.sum"
  for salt in 1 2 3; do
    DARTSCOPE_MUTATION_SEED=$salt DARTSCOPE_MUTATION_ROUNDS=2500 run "hunti_$salt" cargo test --release -p dartscope-index --test robustness_mutations --locked -- --nocapture
    { echo "== seed $salt =="; grep -E '^(panic at|divergence in|test result|error)' "$OUT/hunti_$salt.log" | cut -c1-300 | head -14; } >>"$OUT/hunti.sum"
  done
  emit hunti "$OUT/hunti.sum" --chunk 3900 --max 2
fi
