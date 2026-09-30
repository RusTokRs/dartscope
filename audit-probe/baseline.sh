#!/usr/bin/env bash
# Baseline: toolchain, lockfile diagnosis, state-as-is checks, then a probe-only compile fix and a deeper pass.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"

# --- annotation size experiment (how large may one annotation be?) ---

run env_versions bash -c 'rustc -Vv; cargo -V; rustup show active-toolchain; uname -a; nproc; free -m'
run locked_metadata cargo metadata --locked --no-deps --format-version 1
cp Cargo.lock "$OUT/Cargo.lock.orig"
run update_workspace cargo update --workspace
git diff --no-color Cargo.lock > "$OUT/lock.diff" 2>&1

# ---- pass 1: the repository exactly as committed (only Cargo.lock refreshed) ----
run asis_check cargo check --workspace --all-targets --keep-going --message-format short
run asis_fmt cargo fmt --all -- --check

# ---- pass 2: probe-only compile fix, then everything else ----
run fix_compile "$PY" audit-probe/fix_compile.py
run check_all cargo check --workspace --all-targets --keep-going --message-format short
run check_nodefault cargo check -p dartscope --no-default-features --message-format short
run check_allfeatures cargo check -p dartscope --all-features --message-format short
run fmt_check cargo fmt --all -- --check
run clippy cargo clippy --workspace --all-targets --keep-going --message-format short -- -D warnings
RUSTDOCFLAGS="-D warnings" run doc cargo doc --workspace --no-deps --keep-going --message-format short

for f in env_versions locked_metadata update_workspace fix_compile; do
  emit "$f" "$OUT/$f.log" --chunk 6000 --max 1
done
emit "lock.diff" "$OUT/lock.diff" --chunk 3900 --max 1
emit asis_check "$OUT/asis_check.log" --chunk 3900 --max 3
emit asis_fmt "$OUT/asis_fmt.log" --chunk 3900 --max 3
emit check_all "$OUT/check_all.log" --chunk 3900 --max 4
emit check_nodefault "$OUT/check_nodefault.log" --chunk 3900 --max 1
emit check_allfeatures "$OUT/check_allfeatures.log" --chunk 3900 --max 1
emit fmt_check "$OUT/fmt_check.log" --chunk 3900 --max 4
emit clippy "$OUT/clippy.log" --chunk 3900 --max 6
emit doc "$OUT/doc.log" --chunk 3900 --max 2
