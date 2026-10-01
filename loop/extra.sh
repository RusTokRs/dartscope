# TEMPORARY final-tree gates (removed after they pass): Linux only, for commits tagged [gates].
msg="$("$PY" -c "import json,os;print(json.load(open(os.environ['GITHUB_EVENT_PATH'])).get('head_commit',{}).get('message',''))" 2>/dev/null)"
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[scale\]'; then
  run scale cargo test --release -p dartscope-parse --test adversarial_shapes --locked -- --ignored --nocapture --test-threads=1
  grep -E '^\[|^suspects|^cell|^error|panicked' "$OUT/scale.log" | cut -c1-1500 >"$OUT/scale.sum"
  emit scale "$OUT/scale.sum" --chunk 3900 --max 8
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[fuzz\]'; then
  # The nightly job of ci.yml, step by step (the loop is the only place it can run before hand-off).
  run f_toolchains bash -c 'rustup toolchain install 1.95.0 --profile minimal && rustup toolchain install nightly-2026-07-01 --profile minimal --component rustfmt'
  run f_bridge bash -c 'cargo +1.95.0 test -p dartscope-parse --features fuzzing --locked --quiet && cargo +nightly-2026-07-01 fmt --manifest-path fuzz/Cargo.toml -- --check'
  run f_install cargo +1.95.0 install cargo-fuzz --version 0.13.2 --locked
  run f_targets bash -c 'targets=(lexical_masking directives pubspec_package_config graphql uri_normalization file_analysis); for target in "${targets[@]}"; do echo "### $target"; cargo +nightly-2026-07-01 fuzz build "$target" && cargo +nightly-2026-07-01 fuzz run "$target" -- -runs=256 -max_len=4096 -timeout=5 -rss_limit_mb=2048 || exit 1; done'
  {
    for step in toolchains bridge install targets; do
      echo "== $step: $(tail -n 1 "$OUT/f_$step.log")"
    done
    for step in toolchains bridge install targets; do
      if ! tail -n 1 "$OUT/f_$step.log" | grep -q 'exit=0'; then
        echo "### $step"; grep -vE '^\s*(Compiling|Checking|Downloaded|Downloading|Fresh|Installing|Installed|Updating)' "$OUT/f_$step.log" | tail -n 40 | cut -c1-300
      fi
    done
    echo "--- runs ---"
    grep -E '^### |Done [0-9]+ runs|ERROR|panicked|SUMMARY' "$OUT/f_targets.log" | cut -c1-200
  } >"$OUT/fuzz.sum"
  emit fuzz "$OUT/fuzz.sum" --chunk 3900 --max 4
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[fuzz-long\]'; then
  # One longer coverage-guided campaign over the whole file analysis (a hunt, not a gate).
  run h_toolchains bash -c 'rustup toolchain install 1.95.0 --profile minimal && rustup toolchain install nightly-2026-07-01 --profile minimal --component rustfmt'
  run h_install cargo +1.95.0 install cargo-fuzz --version 0.13.2 --locked
  run h_fuzz bash -c 'cargo +nightly-2026-07-01 fuzz build file_analysis && cargo +nightly-2026-07-01 fuzz run file_analysis -- -max_total_time=1080 -max_len=4096 -timeout=10 -rss_limit_mb=2048 -print_final_stats=1; code=$?; echo "fuzz exit code: $code"; for f in fuzz/artifacts/file_analysis/*; do [ -f "$f" ] && { echo "### artifact $f"; head -c 4000 "$f" | cat -v | head -120; }; done; exit 0'
  {
    echo "== toolchains: $(tail -n 1 "$OUT/h_toolchains.log")"
    echo "== install: $(tail -n 1 "$OUT/h_install.log")"
    echo "== fuzz: $(tail -n 1 "$OUT/h_fuzz.log")"
    grep -E 'fuzz exit code|^### artifact|panicked|ERROR|SUMMARY|stat::|Done [0-9]+ runs|cov:.*exec/s' "$OUT/h_fuzz.log" | tail -n 40 | cut -c1-300
    echo "--- artifacts ---"
    sed -n '/^### artifact/,$p' "$OUT/h_fuzz.log" | head -n 200 | cut -c1-300
  } >"$OUT/fuzzlong.sum"
  emit fuzzlong "$OUT/fuzzlong.sum" --chunk 3900 --max 6
fi
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[uri\]'; then
  # The one-off comparison of crate::uri with the uriparse crate that it replaces.
  run u_diff cargo test -p dartscope-resolve --lib --locked -- --ignored --nocapture differential
  grep -E '^differential|panicked|error' "$OUT/u_diff.log" | cut -c1-600 >"$OUT/uri.sum"
  emit uri "$OUT/uri.sum" --chunk 3900 --max 6
fi
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
