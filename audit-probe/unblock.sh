#!/usr/bin/env bash
# Verifies that the candidate minimal "unblock" patch turns the workspace green (LSP handled separately).
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"

run apply_patch git apply --verbose audit-probe/unblock.patch
if [ -f audit-probe/unblock-lsp.patch ]; then run apply_lsp_patch git apply --verbose audit-probe/unblock-lsp.patch; fi
run fmt_apply cargo fmt --all
git diff --stat > "$OUT/after_fmt.stat" 2>&1
run fmt_verify cargo fmt --all -- --check
EXCLUDE=""
if [ ! -f audit-probe/unblock-lsp.patch ]; then EXCLUDE="--exclude dartscope-lsp"; fi
run check_locked cargo check --workspace $EXCLUDE --all-targets --locked --message-format short
run clippy_locked cargo clippy --workspace $EXCLUDE --all-targets --locked --message-format short -- -D warnings
run test_locked cargo test --workspace $EXCLUDE --locked --no-fail-fast
RUSTDOCFLAGS="-D warnings" run doc_locked cargo doc --workspace $EXCLUDE --no-deps --locked --message-format short
run unittests "$PY" -m unittest discover -s tools/tests -p 'test_*.py'
run umbrella_all_features cargo check -p dartscope --all-features --locked --message-format short
run umbrella_min cargo check -p dartscope --no-default-features --locked --message-format short
run package_count bash -c 'cargo package --workspace --locked --allow-dirty --no-verify >/dev/null 2>&1; ls target/package/dartscope-*.crate | wc -l'

emit apply_patch "$OUT/apply_patch.log" --chunk 1500 --max 1 --tail
emit fmt_stat "$OUT/after_fmt.stat" --chunk 3900 --max 1
emit fmt_verify "$OUT/fmt_verify.log" --chunk 1500 --max 1 --tail
emit check_locked "$OUT/check_locked.log" --chunk 3900 --max 1 --tail
emit clippy_locked "$OUT/clippy_locked.log" --chunk 3900 --max 3
grep -E '^(test result:|test .* FAILED|error|thread .* panicked)' "$OUT/test_locked.log" > "$OUT/test_locked.sum"
{ echo "--- totals ---"; grep -E '^test result:' "$OUT/test_locked.log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'; } >> "$OUT/test_locked.sum"
emit test_locked "$OUT/test_locked.sum" --chunk 3900 --max 2
emit doc_locked "$OUT/doc_locked.log" --chunk 2500 --max 1 --tail
emit unittests "$OUT/unittests.log" --chunk 800 --max 1 --tail
emit umbrella_all_features "$OUT/umbrella_all_features.log" --chunk 2500 --max 1 --tail
emit umbrella_min "$OUT/umbrella_min.log" --chunk 1200 --max 1 --tail
emit package_count "$OUT/package_count.log" --chunk 300 --max 1
