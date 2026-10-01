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
