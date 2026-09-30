#!/usr/bin/env bash
# Baseline: toolchain, lockfile diagnosis, check/fmt/clippy/doc/feature matrix.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"

# --- annotation size experiment (how large may one annotation be?) ---
"$PY" - <<'PY' >"$OUT/sizes.txt"
import string
for size in (30_000, 60_000, 120_000):
    body = ("".join(string.ascii_lowercase[i % 26] for i in range(size - 20)))
    print(f"::notice title=size-test {size}::SIZE={size} " + body)
PY
cat "$OUT/sizes.txt"

run env_versions bash -c 'rustc -Vv; cargo -V; rustup show active-toolchain; uname -a; nproc; free -m'
run locked_metadata cargo metadata --locked --no-deps --format-version 1
cp Cargo.lock "$OUT/Cargo.lock.orig"
run update_workspace cargo update --workspace
git diff --no-color Cargo.lock > "$OUT/lock.diff" 2>&1
run locked_after_update cargo metadata --locked --no-deps --format-version 1 --quiet

run check_all cargo check --workspace --all-targets --keep-going --message-format short
run check_nodefault cargo check -p dartscope --no-default-features --message-format short
run check_allfeatures cargo check -p dartscope --all-features --message-format short
run fmt_check cargo fmt --all -- --check
run clippy cargo clippy --workspace --all-targets --keep-going --message-format short -- -D warnings
RUSTDOCFLAGS="-D warnings" run doc cargo doc --workspace --no-deps --keep-going --message-format short

for f in env_versions locked_metadata update_workspace locked_after_update; do
  emit "$f" "$OUT/$f.log" --chunk 6000 --max 1
done
emit "lock.diff" "$OUT/lock.diff" --chunk 8000 --max 1
emit check_all "$OUT/check_all.log" --chunk 22000 --max 3
emit check_nodefault "$OUT/check_nodefault.log" --chunk 8000 --max 1
emit check_allfeatures "$OUT/check_allfeatures.log" --chunk 12000 --max 1
emit fmt_check "$OUT/fmt_check.log" --chunk 22000 --max 3
emit clippy "$OUT/clippy.log" --chunk 22000 --max 4
emit doc "$OUT/doc.log" --chunk 12000 --max 2
