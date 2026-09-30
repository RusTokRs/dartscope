#!/usr/bin/env bash
# LSP probe: unpatched compile errors, patched build, clippy, unit tests, wire-level behaviour.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
"$PY" audit-probe/fix_compile.py

run lsp_check_unpatched cargo check -p dartscope-lsp --all-targets --message-format short
emit lsp_check_unpatched "$OUT/lsp_check_unpatched.log" --chunk 3900 --max 2
run lsp_patch "$PY" audit-probe/lsp_patch.py
cat "$OUT/lsp_patch.log"
run lsp_check_patched cargo check -p dartscope-lsp --all-targets --message-format short
emit lsp_check_patched "$OUT/lsp_check_patched.log" --chunk 3900 --max 2
run lsp_clippy cargo clippy -p dartscope-lsp --all-targets --message-format short -- -D warnings
emit lsp_clippy "$OUT/lsp_clippy.log" --chunk 3900 --max 4
run lsp_tests cargo test -p dartscope-lsp --no-fail-fast
emit lsp_tests_tail "$OUT/lsp_tests.log" --chunk 3900 --max 3 --tail
run lsp_build cargo build -p dartscope-lsp --bin dartscope-lsp --message-format short
if [ -x "target/debug/dartscope-lsp" ]; then
  run lsp_wire "$PY" audit-probe/lsp_probe.py target/debug/dartscope-lsp
  emit lsp_wire "$OUT/lsp_probe.txt" --chunk 3900 --max 8
else
  echo "dartscope-lsp binary missing"; emit lsp_build "$OUT/lsp_build.log" --chunk 3900 --max 2 --tail
fi
