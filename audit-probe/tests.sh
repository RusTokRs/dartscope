#!/usr/bin/env bash
# Full test run (workspace minus LSP, then LSP on its own), without --locked.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
"$PY" audit-probe/fix_compile.py

run test_workspace cargo test --workspace --exclude dartscope-lsp --no-fail-fast

# ---- audit probe tests (copied in, never part of the repository diff) ----
cp audit-probe/rust-tests/audit_probe_navigation.rs crates/dartscope-index/tests/
cp audit-probe/rust-tests/audit_probe_incremental.rs crates/dartscope-index/tests/
cp audit-probe/rust-tests/audit_probe_flutter.rs crates/dartscope-flutter/tests/
cp audit-probe/rust-tests/audit_probe_parse.rs crates/dartscope-parse/tests/
run probe_index cargo test -p dartscope-index --test audit_probe_navigation --test audit_probe_incremental --no-fail-fast -- --nocapture --test-threads=1
run probe_flutter cargo test -p dartscope-flutter --test audit_probe_flutter --no-fail-fast -- --nocapture
run probe_parse cargo test -p dartscope-parse --test audit_probe_parse --no-fail-fast -- --nocapture --test-threads=1
for f in probe_index probe_flutter probe_parse; do
  { grep -E '^(AUDIT|test |error)' "$OUT/$f.log"; grep -A8 'panicked at' "$OUT/$f.log"; } | head -150 > "$OUT/$f.summary"
  emit "$f" "$OUT/$f.summary" --chunk 9000 --max 1
  emit "$f tail" "$OUT/$f.log" --chunk 9000 --max 1 --tail
done
run test_lsp cargo test -p dartscope-lsp --no-fail-fast
run test_fuzzing_feature cargo test -p dartscope-parse --features fuzzing --no-fail-fast
run test_umbrella_allfeatures_nolsp cargo test -p dartscope --features parse,resolve,index,lints,json,flutter --no-fail-fast

for f in test_workspace test_lsp test_fuzzing_feature test_umbrella_allfeatures_nolsp; do
  grep -E '^(test result:|failures:|error(\[|:)|warning: unused|thread .* panicked|test .* FAILED|     Running|   Doc-tests)' "$OUT/$f.log" > "$OUT/$f.summary" 2>&1 || true
  { echo "--- panic context ---"; grep -A7 'panicked at' "$OUT/$f.log" | head -120; } >> "$OUT/$f.summary" 2>&1 || true
  {
    echo "--- totals ---"
    grep -E '^test result:' "$OUT/$f.log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'
    echo "--- failed tests ---"
    grep -E '^test .* FAILED$' "$OUT/$f.log" || true
    echo "--- compile errors ---"
    grep -E '^error' "$OUT/$f.log" | head -60 || true
  } > "$OUT/$f.totals"
  emit "$f totals" "$OUT/$f.totals" --chunk 12000 --max 1
  emit "$f tail" "$OUT/$f.log" --chunk 22000 --max 2 --tail
  emit "$f summary" "$OUT/$f.summary" --chunk 12000 --max 1
done
