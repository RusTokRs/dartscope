#!/usr/bin/env bash
# Where does analyze-file spend its time? callgrind profile at two input sizes (temporary audit probe).
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
run apply_unblock git apply --verbose docs/development/audit-2026-09-30-unblock.patch
run build_cli cargo build --release -p dartscope-cli --locked --message-format short
BIN="$GITHUB_WORKSPACE/target/release/dartscope"
if [ ! -x "$BIN" ]; then emit build_cli "$OUT/build_cli.log" --chunk 3000 --max 1 --tail; exit 0; fi
run apt_valgrind bash -c 'sudo apt-get update -qq && sudo apt-get install -y -qq valgrind'
if ! command -v valgrind >/dev/null; then emit apt_valgrind "$OUT/apt_valgrind.log" --chunk 3000 --max 1 --tail; exit 0; fi
valgrind --version > "$OUT/vgver.txt" 2>&1
for n in 600 1200; do
  d="$RUNNER_TEMP/prof$n"; mkdir -p "$d/lib"
  "$PY" - "$n" "$d/lib/a.dart" <<'PY'
import sys
n = int(sys.argv[1])
open(sys.argv[2], "w").write("".join(
    f"class C{i} {{\n  final int f{i};\n  C{i}(this.f{i});\n  int m{i}(int a) => a + f{i};\n}}\n" for i in range(n)))
PY
  ( cd "$d" && run "callgrind_$n" valgrind --tool=callgrind --callgrind-out-file="$OUT/cg$n.out" "$BIN" analyze-file lib/a.dart )
  run "annotate_$n" callgrind_annotate "$OUT/cg$n.out"
  { echo "== n=$n classes: top of callgrind_annotate (exclusive Ir) =="; sed -n 1,40p "$OUT/annotate_$n.log" | cut -c1-210; } > "$OUT/top_$n.txt"
  emit "profile_$n" "$OUT/top_$n.txt" --chunk 3900 --max 1
done
{ echo "valgrind: $(cat "$OUT/vgver.txt")"; grep -h "Collected" "$OUT/annotate_600.log" "$OUT/annotate_1200.log"; } > "$OUT/collected.txt"
emit profile_totals "$OUT/collected.txt" --chunk 1000 --max 1
