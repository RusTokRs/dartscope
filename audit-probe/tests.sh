#!/usr/bin/env bash
# Full test run (workspace minus LSP, then LSP on its own), without --locked.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1

run test_workspace cargo test --workspace --exclude dartscope-lsp --no-fail-fast
run test_lsp cargo test -p dartscope-lsp --no-fail-fast
run test_fuzzing_feature cargo test -p dartscope-parse --features fuzzing --no-fail-fast
run test_umbrella_allfeatures_nolsp cargo test -p dartscope --features parse,resolve,index,lints,json,flutter --no-fail-fast

for f in test_workspace test_lsp test_fuzzing_feature test_umbrella_allfeatures_nolsp; do
  grep -E '^(test result:|failures:|error(\[|:)|warning: unused|thread .* panicked|test .* FAILED|     Running|   Doc-tests)' "$OUT/$f.log" > "$OUT/$f.summary" 2>&1 || true
  {
    echo "--- totals ---"
    grep -E '^test result:' "$OUT/$f.log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'
    echo "--- failed tests ---"
    grep -E '^test .* FAILED$' "$OUT/$f.log" || true
    echo "--- compile errors ---"
    grep -E '^error' "$OUT/$f.log" | head -60 || true
  } > "$OUT/$f.totals"
  emit "$f totals" "$OUT/$f.totals" --chunk 12000 --max 1
  emit "$f tail" "$OUT/$f.log" --chunk 22000 --max 3 --tail
  emit "$f summary" "$OUT/$f.summary" --chunk 12000 --max 2
done
