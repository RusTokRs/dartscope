#!/usr/bin/env bash
# Wire-level LSP probe against the server built from the unblock patch (temporary audit probe).
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
run apply_unblock git apply --verbose docs/development/audit-2026-09-30-unblock.patch
run lsp_build cargo build -p dartscope-lsp --bin dartscope-lsp --locked --message-format short
if [ -x target/debug/dartscope-lsp ]; then
  run lsp_wire perl -e 'alarm shift; exec @ARGV' 900 "$PY" audit-probe/lsp_probe.py target/debug/dartscope-lsp
  emit lsp_wire "$OUT/lsp_wire.log" --chunk 3900 --max 10
else
  emit lsp_build "$OUT/lsp_build.log" --chunk 3900 --max 2 --tail
fi
