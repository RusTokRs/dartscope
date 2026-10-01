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

# TEMPORARY callgrind profile for commits tagged [profile] (Linux only).
if [ "${RUNNER_OS:-}" = "Linux" ] && printf '%s' "$msg" | grep -q '\[profile\]'; then
  CARGO_PROFILE_RELEASE_DEBUG=1 run prof_build cargo build --release -p dartscope-cli --locked --message-format short
  PROF_BIN="$GITHUB_WORKSPACE/target/release/dartscope"
  if [ ! -x "$PROF_BIN" ]; then
    emit prof_build "$OUT/prof_build.log" --chunk 3000 --max 1 --tail
  else
    run apt_valgrind bash -c 'sudo apt-get update -qq && sudo apt-get install -y -qq valgrind'
    if ! command -v valgrind >/dev/null; then
      emit apt_valgrind "$OUT/apt_valgrind.log" --chunk 3000 --max 1 --tail
    else
      for spec in functions:8000 classes:4000 call_arguments:3000; do
        name="${spec%%:*}"; n="${spec##*:}"
        d="$RUNNER_TEMP/prof-$name"; mkdir -p "$d/lib"
        "$PY" -c "import sys; sys.path.insert(0, 'loop'); import perf; open(sys.argv[1], 'w').write(getattr(perf, sys.argv[2])(int(sys.argv[3])))" "$d/lib/a.dart" "$name" "$n"
        ( cd "$d" && run "cg_$name" valgrind --tool=callgrind --callgrind-out-file="$OUT/cg_$name.out" "$PROF_BIN" analyze-file lib/a.dart )
        run "ann_$name" callgrind_annotate --inclusive=yes "$OUT/cg_$name.out"
        { echo "== $name n=$n inclusive Ir =="; sed -n '/file:function/,$p' "$OUT/ann_$name.log" | sed -n 2,30p | cut -c1-190; } > "$OUT/top_$name.txt"
        emit "prof_$name" "$OUT/top_$name.txt" --chunk 3900 --max 1
      done
    fi
  fi
fi
