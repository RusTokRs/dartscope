#!/usr/bin/env bash
# Runtime probe: release CLI, Dart battery, project traversal, CLI behaviour, scaling, fuzzing, real-world corpus.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
"$PY" audit-probe/fix_compile.py

run build_cli cargo build --release -p dartscope-cli --message-format short
emit build_cli "$OUT/build_cli.log" --chunk 3900 --max 1 --tail
if [ ! -x "target/release/dartscope" ]; then echo "CLI did not build"; exit 0; fi

CORPUS="$RUNNER_TEMP/corpus"; mkdir -p "$CORPUS"
for repo in dart-lang/shelf felangel/bloc rrousselGit/riverpod flutter/samples; do
  name="${repo#*/}"
  git clone --depth 1 --quiet "https://github.com/$repo.git" "$CORPUS/$name" 2>>"$OUT/clone.log" || echo "clone failed: $repo" >>"$OUT/clone.log"
done
export CORPUS_DIRS="$(ls -d "$CORPUS"/* | tr '\n' ':' | sed 's/:$//')"
echo "corpus: $CORPUS_DIRS"

run rt_battery "$PY" audit-probe/runtime.py battery
run rt_projects "$PY" audit-probe/runtime.py projects
run rt_cli "$PY" audit-probe/runtime.py cli
emit "battery" "$OUT/runtime_battery.txt" --chunk 3900 --max 14
emit "projects" "$OUT/runtime_projects.txt" --chunk 3900 --max 8
emit "cli" "$OUT/runtime_cli.txt" --chunk 3900 --max 5
