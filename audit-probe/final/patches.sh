#!/usr/bin/env bash
# Verifies the three candidate patches from docs/development/ on a clean checkout (temporary audit probe).
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
P=docs/development

summarize_tests() {
  local log="$OUT/$1.log"
  {
    echo "binaries with results: $(grep -cE '^test result:' "$log")"
    grep -E '^test result: FAILED|^test .* FAILED$|thread .* panicked at|^error(\[|:)' "$log" | cut -c1-240
    echo "--- totals ---"
    grep -E '^test result:' "$log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'
  } > "$OUT/$1.sum"
}
summarize_clippy() {
  grep -E '^(crates|tools)/.*: (error|warning)|^(error|warning)(\[|:)' "$OUT/$1.log" | cut -c1-260 > "$OUT/$1.sum"
  echo "[exit=$(grep -o 'exit=[0-9]*' "$OUT/$1.log" | tail -1)]" >> "$OUT/$1.sum"
}
summarize_specs() {
  grep -E '^(     Running|test |test result:|thread .* panicked at|error(\[|:))' "$OUT/$1.log" | cut -c1-240 > "$OUT/$1.sum"
}

run apply_unblock git apply --verbose "$P/audit-2026-09-30-unblock.patch"
run apply_clippy git apply --verbose "$P/audit-2026-09-30-clippy-followup.patch"
run apply_lsptests git apply --verbose "$P/audit-2026-09-30-lsp-test-fixes.patch"
run fmt_apply cargo fmt --all
run fmt_check cargo fmt --all -- --check
run clippy_1 cargo clippy --workspace --all-targets --locked --keep-going --message-format short -- -D warnings
run test_1 cargo test --workspace --locked --no-fail-fast
emit apply_unblock "$OUT/apply_unblock.log" --chunk 500 --max 1 --tail
emit apply_clippy "$OUT/apply_clippy.log" --chunk 500 --max 1 --tail
emit apply_lsptests "$OUT/apply_lsptests.log" --chunk 500 --max 1 --tail
emit fmt_check_1 "$OUT/fmt_check.log" --chunk 800 --max 1 --tail
summarize_clippy clippy_1
emit clippy_1 "$OUT/clippy_1.sum" --chunk 3900 --max 1
summarize_tests test_1
emit test_1 "$OUT/test_1.sum" --chunk 3900 --max 1

run apply_reg git apply --verbose "$P/audit-2026-09-30-regression-tests.patch"
run fmt_apply2 cargo fmt --all
run fmt_check2 cargo fmt --all -- --check
run clippy_2 cargo clippy --workspace --all-targets --locked --keep-going --message-format short -- -D warnings
run test_2 cargo test --workspace --locked --no-fail-fast
emit apply_reg "$OUT/apply_reg.log" --chunk 600 --max 1 --tail
emit fmt_check_2 "$OUT/fmt_check2.log" --chunk 800 --max 1 --tail
summarize_clippy clippy_2
emit clippy_2 "$OUT/clippy_2.sum" --chunk 3900 --max 1
summarize_tests test_2
emit test_2 "$OUT/test_2.sum" --chunk 3900 --max 1

run spec_index cargo test --locked -p dartscope-index --test audit_cycles_and_deep_chains --test audit_incremental_snapshot_equivalence --test audit_navigation_extensions_and_inheritance --no-fail-fast -- --include-ignored
run spec_flutter cargo test --locked -p dartscope-flutter --test audit_extension_is_not_a_widget --no-fail-fast -- --include-ignored
run spec_parse cargo test --locked -p dartscope-parse --test audit_bom_and_inventory_gaps --no-fail-fast -- --include-ignored
for spec in spec_index spec_flutter spec_parse; do
  summarize_specs "$spec"
  emit "$spec" "$OUT/$spec.sum" --chunk 3900 --max 1
done

# Repository gates on a clean, git-initialised copy of the patched tree (without the probe files).
CLEAN="$RUNNER_TEMP/clean"; rm -rf "$CLEAN"; mkdir -p "$CLEAN"
git ls-files -z --cached --others --exclude-standard | grep -zv -e '^audit-probe/' -e '^.github/workflows/audit-probe' | xargs -0 tar -cf - | tar -xf - -C "$CLEAN"
( cd "$CLEAN" && git init -q && git add -A && run consistency "$PY" tools/check-repository-consistency.py )
( cd "$CLEAN" && run workflow_policy "$PY" tools/check-workflow-policy.py )
( cd "$CLEAN" && run dependency_policy "$PY" tools/check-dependency-policy.py )
( cd "$CLEAN" && run pyunit "$PY" -m unittest discover -s tools/tests )
{ for n in consistency workflow_policy dependency_policy pyunit; do echo "--- $n"; tail -c 700 "$OUT/$n.log"; echo; done; } > "$OUT/gates.sum"
emit gates "$OUT/gates.sum" --chunk 3900 --max 1
