#!/usr/bin/env bash
# Runtime probe part 2 (long sections): scaling, fuzzing, corpus.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
run build_cli cargo build --release -p dartscope-cli --message-format short
if [ ! -x "target/release/dartscope" ]; then echo "CLI did not build"; emit build_cli "$OUT/build_cli.log" --chunk 20000 --max 2 --tail; exit 0; fi

CORPUS="$RUNNER_TEMP/corpus"; mkdir -p "$CORPUS"
for repo in dart-lang/shelf felangel/bloc rrousselGit/riverpod flutter/samples; do
  name="${repo#*/}"
  git clone --depth 1 --quiet "https://github.com/$repo.git" "$CORPUS/$name" 2>>"$OUT/clone.log" || echo "clone failed: $repo" >>"$OUT/clone.log"
done
export CORPUS_DIRS="$(ls -d "$CORPUS"/* | tr '\n' ':' | sed 's/:$//')"
echo "corpus: $CORPUS_DIRS"

run rt_perf "$PY" audit-probe/runtime.py perf
emit "perf" "$OUT/runtime_perf.txt" --chunk 22000 --max 1
FUZZ_SECONDS=300 run rt_fuzz "$PY" audit-probe/runtime.py fuzz
emit "fuzz" "$OUT/runtime_fuzz.txt" --chunk 22000 --max 3
run rt_corpus "$PY" audit-probe/runtime.py corpus
emit "corpus" "$OUT/runtime_corpus.txt" --chunk 22000 --max 2
emit "clone log" "$OUT/clone.log" --chunk 3000 --max 1
