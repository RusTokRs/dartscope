#!/usr/bin/env bash
# Extra probes: repository release gates after compile fixes, and cycle/deep-chain robustness.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
"$PY" audit-probe/fix_compile.py

cp audit-probe/rust-tests/audit_probe_cycles.rs crates/dartscope-index/tests/
run probe_cycles cargo test -p dartscope-index --test audit_probe_cycles --no-fail-fast -- --nocapture --test-threads=1
{ grep -E '^(AUDIT|test |error)' "$OUT/probe_cycles.log"; grep -A8 'panicked at' "$OUT/probe_cycles.log"; grep -iE 'overflow|SIGSEGV|SIGABRT|signal' "$OUT/probe_cycles.log"; } | head -120 > "$OUT/probe_cycles.summary"
emit probe_cycles "$OUT/probe_cycles.summary" --chunk 3900 --max 3
emit probe_cycles_tail "$OUT/probe_cycles.log" --chunk 3900 --max 1 --tail

run gate_consistency "$PY" tools/check-repository-consistency.py
run gate_release_packages "$PY" tools/check-release-packages.py
run gate_publish_syntax bash -n tools/publish-crates.sh
run gate_unittests "$PY" -m unittest discover -s tools/tests -p 'test_*.py'
run gate_benchmark_package_count bash -c 'cargo package --workspace --allow-dirty --no-verify >/dev/null 2>&1; ls target/package/dartscope-*.crate | wc -l'
for g in gate_consistency gate_release_packages gate_publish_syntax gate_unittests gate_benchmark_package_count; do
  emit "$g" "$OUT/$g.log" --chunk 3900 --max 1 --tail
done
