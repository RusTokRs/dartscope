#!/usr/bin/env bash
# Do the patches apply on a default Windows checkout (text=auto -> CRLF for files without eol=lf)? (temporary audit probe)
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
P=docs/development
git config --show-origin --get-all core.autocrlf > "$OUT/cfg.txt" 2>&1 || true
git config --show-origin --get-all core.eol >> "$OUT/cfg.txt" 2>&1 || true
git ls-files --eol tools/report_benchmark_regressions.py tools/tests/test_macos_portability_policy.py crates/dartscope-lsp/src/server.rs Cargo.lock .github/workflows/ci.yml > "$OUT/eol.txt" 2>&1
run plain_check git apply --check "$P/audit-2026-09-30-unblock.patch"
run ws_apply_1 git apply --verbose --ignore-whitespace "$P/audit-2026-09-30-unblock.patch"
run ws_apply_2 git apply --verbose --ignore-whitespace "$P/audit-2026-09-30-clippy-followup.patch"
run ws_apply_3 git apply --verbose --ignore-whitespace "$P/audit-2026-09-30-lsp-test-fixes.patch"
run ws_apply_4 git apply --verbose --ignore-whitespace "$P/audit-2026-09-30-regression-tests.patch"
{ echo "--- core.* config"; cat "$OUT/cfg.txt"; echo "--- eol"; cat "$OUT/eol.txt"; echo "--- plain git apply --check (unblock)"; head -c 900 "$OUT/plain_check.log";
  for n in ws_apply_1 ws_apply_2 ws_apply_3 ws_apply_4; do echo "--- $n"; grep -E 'error|exit=' "$OUT/$n.log" | head -5; done; } > "$OUT/applycheck.sum"
emit applycheck "$OUT/applycheck.sum" --chunk 3900 --max 1
