#!/usr/bin/env bash
# TEMPORARY development loop (removed before hand-off): format (+push), clippy, tests, optional extras.
source "$GITHUB_WORKSPACE/loop/lib.sh"
cd "$GITHUB_WORKSPACE"

run fmt_apply cargo fmt --all
if [ "${LOOP_PUSH:-0}" = "1" ] && ! git diff --quiet -- crates tools fuzz 2>/dev/null; then
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  git add -u crates tools fuzz
  git commit -q -m "style: cargo fmt (CI loop)"
  git push origin "HEAD:${GITHUB_REF_NAME}" >"$OUT/push.log" 2>&1
  echo "[push exit=$?]" >>"$OUT/push.log"
  emit fmt_push "$OUT/push.log" --chunk 800 --max 1 --tail
fi

run clippy cargo clippy --workspace --all-targets --locked --keep-going --message-format short -- -D warnings
{
  grep -E '^(crates|tools|fuzz)[/\\].*: (error|warning)|^(error|warning)(\[|:)' "$OUT/clippy.log" \
    | grep -v -e 'warnings emitted' -e 'aborting due to' -e 'could not compile' | cut -c1-320
  grep -o '\[exit=[0-9]* elapsed=[0-9]*s\]' "$OUT/clippy.log" | tail -1
} >"$OUT/clippy.sum"
emit clippy "$OUT/clippy.sum" --chunk 3900 --max 2

if grep -q 'error\[E' "$OUT/clippy.log"; then
  echo "tests skipped: compile errors" >"$OUT/test.sum"
else
  run test cargo test --workspace --locked --no-fail-fast
  {
    echo "binaries with results: $(grep -cE '^test result:' "$OUT/test.log")"
    grep -E '^test result: FAILED|^test .* FAILED$|^error(\[|:)' "$OUT/test.log" | cut -c1-300 | head -60
    echo "--- totals ---"
    grep -E '^test result:' "$OUT/test.log" | awk '{p+=$4; f+=$6; i+=$8} END {print "passed=" p " failed=" f " ignored=" i}'
    echo "--- failure details ---"
    awk '/^---- .* stdout ----$/{p=1} /^failures:$/{p=0} p' "$OUT/test.log" | cut -c1-300 | head -c 7000
  } >"$OUT/test.sum"
fi
emit tests "$OUT/test.sum" --chunk 3900 --max 3

if [ "${LOOP_FULL:-0}" = "1" ]; then
  run check_features bash -c 'cargo check -p dartscope --no-default-features --locked && cargo check -p dartscope --all-features --locked'
  RUSTDOCFLAGS="-D warnings" run doc cargo doc --workspace --no-deps --locked
  { tail -c 600 "$OUT/check_features.log"; echo; tail -c 600 "$OUT/doc.log"; } >"$OUT/full.sum"
  emit full "$OUT/full.sum" --chunk 3000 --max 1
fi

if [ -f "$GITHUB_WORKSPACE/loop/extra.sh" ]; then
  # shellcheck disable=SC1091
  source "$GITHUB_WORKSPACE/loop/extra.sh"
fi
