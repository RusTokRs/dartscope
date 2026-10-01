# TEMPORARY perf measurements (removed before hand-off): Linux only, for commits tagged [perf].
msg="$("$PY" -c "import json,os;print(json.load(open(os.environ['GITHUB_EVENT_PATH'])).get('head_commit',{}).get('message',''))" 2>/dev/null)"
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[perf\]'; then
  run perf_phase cargo test --release -p dartscope-parse --lib zz_ --locked -- --ignored --nocapture --test-threads=1
  grep -E '^(phase|test result|error)' "$OUT/perf_phase.log" > "$OUT/perf_phase.sum"
  emit perf_phase "$OUT/perf_phase.sum" --chunk 3900 --max 3
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
    DARTSCOPE_MUTATION_SEED=$salt DARTSCOPE_MUTATION_ROUNDS=1500 run "hunti_$salt" cargo test --release -p dartscope-index --test robustness_mutations --locked -- --nocapture
    { echo "== seed $salt =="; grep -E '^(panic at|divergence in|test result|error)' "$OUT/hunti_$salt.log" | cut -c1-300 | head -14; } >>"$OUT/hunti.sum"
  done
  emit hunti "$OUT/hunti.sum" --chunk 3900 --max 2
fi
# TEMPORARY final-tree gates (removed after they pass): Linux only, for commits tagged [gates].
msg="$("$PY" -c "import json,os;print(json.load(open(os.environ['GITHUB_EVENT_PATH'])).get('head_commit',{}).get('message',''))" 2>/dev/null)"
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[gates\]'; then
  # Emulate the final tree: the loop and its workflow are not part of it.
  cp -r loop "$RUNNER_TEMP/loop-copy"
  emit() { "$PY" "$RUNNER_TEMP/loop-copy/emit.py" "$@"; }
  rm -rf loop .github/workflows/dev-loop.yml
  run g_fmt cargo fmt --all -- --check
  run g_consistency python3 tools/check-repository-consistency.py
  run g_policy python3 tools/check-workflow-policy.py
  run g_unittest python3 -m unittest discover -s tools/tests -p 'test_*.py'
  run g_deps python3 tools/check-dependency-policy.py
  run g_actionlint bash -c 'go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 && "$(go env GOPATH)/bin/actionlint"'
  run g_check cargo check --workspace --all-targets --locked
  run g_test cargo test --workspace --locked --quiet
  run g_clippy cargo clippy --workspace --all-targets --locked -- -D warnings
  RUSTDOCFLAGS="-D warnings" run g_doc cargo doc --workspace --no-deps --locked
  run g_features bash -c 'cargo check -p dartscope --no-default-features --locked && cargo check -p dartscope --all-features --locked'
  run g_fuzzbridge cargo test -p dartscope-parse --features fuzzing --locked --quiet
  run g_package bash -c 'cargo package --workspace --locked --allow-dirty --no-verify && test "$(find target/package -maxdepth 1 -type f -name "dartscope-*.crate" | wc -l | tr -d " ")" = 10'
  run g_machete bash -c 'cargo install cargo-machete --version 0.9.2 --locked && cargo machete'
  {
    for gate in fmt consistency policy unittest deps actionlint check test clippy doc features fuzzbridge package machete; do
      echo "== $gate: $(tail -n 1 "$OUT/g_$gate.log")"
    done
    echo "--- test totals ---"
    grep -E '^test result:' "$OUT/g_test.log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'
    echo "--- failures ---"
    for gate in fmt consistency policy unittest deps actionlint check test clippy doc features fuzzbridge package machete; do
      if ! tail -n 1 "$OUT/g_$gate.log" | grep -q 'exit=0'; then
        echo "### $gate"; grep -vE '^\s*(Compiling|Checking|Documenting|Downloaded|Downloading|Fresh|Packaging|Packaged|Verifying)' "$OUT/g_$gate.log" | tail -n 25 | cut -c1-300
      fi
    done
  } >"$OUT/gates.sum"
  emit gates "$OUT/gates.sum" --chunk 3900 --max 3
fi
