#!/usr/bin/env bash
# Differential probe: DartScope vs the official Dart parser on real-world repositories.
source "$GITHUB_WORKSPACE/audit-probe/lib.sh"
cd "$GITHUB_WORKSPACE"
cargo update --workspace >/dev/null 2>&1
"$PY" audit-probe/fix_compile.py
run build_cli cargo build --release -p dartscope-cli --message-format short
if [ ! -x target/release/dartscope ]; then emit build_cli "$OUT/build_cli.log" --chunk 3900 --max 2 --tail; exit 0; fi

# --- Dart SDK ---
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) SDK=macos-arm64 ;;
  Linux-x86_64) SDK=linux-x64 ;;
  *) SDK=linux-x64 ;;
esac
SDKDIR="$RUNNER_TEMP/dart"; mkdir -p "$SDKDIR"
curl -fsSL "https://storage.googleapis.com/dart-archive/channels/stable/release/latest/sdk/dartsdk-$SDK-release.zip" -o "$SDKDIR/sdk.zip" 2>"$OUT/sdk_download.log" || echo "sdk download failed" >>"$OUT/sdk_download.log"
(cd "$SDKDIR" && unzip -q sdk.zip) 2>>"$OUT/sdk_download.log"
export PATH="$SDKDIR/dart-sdk/bin:$PATH"
run dart_version dart --version
run dart_pub_get bash -c 'cd audit-probe/diff && dart pub get'
run dart_compile bash -c 'cd audit-probe/diff && dart compile exe bin/decls.dart -o "$RUNNER_TEMP/decls" 2>&1 || dart analyze bin/decls.dart 2>&1 | head -40'
emit dart_setup "$OUT/dart_version.log" --chunk 1200 --max 1
emit dart_pub "$OUT/dart_pub_get.log" --chunk 2500 --max 1 --tail
emit dart_compile "$OUT/dart_compile.log" --chunk 3500 --max 1 --tail
if [ ! -x "$RUNNER_TEMP/decls" ]; then echo "reference extractor did not compile"; exit 0; fi

# --- corpus ---
CORPUS="$RUNNER_TEMP/corpus"; mkdir -p "$CORPUS"
for repo in dart-lang/shelf felangel/bloc rrousselGit/riverpod dart-lang/http flutter/samples; do
  git clone --depth 1 --quiet "https://github.com/$repo.git" "$CORPUS/${repo#*/}" 2>>"$OUT/clone.log" || echo "clone failed: $repo" >>"$OUT/clone.log"
done
for repo in shelf bloc riverpod http samples; do
  find "$CORPUS/$repo" -name '*.dart' -size -200k -not -path '*/.git/*' -not -name '*.g.dart' -not -name '*.freezed.dart' 2>/dev/null | sort | head -1200 > "$OUT/files_$repo.txt"
  echo "$repo: $(wc -l < "$OUT/files_$repo.txt") files"
  run "ref_$repo" "$RUNNER_TEMP/decls" "$OUT/files_$repo.txt" "$OUT/ref_$repo.json"
  run "cmp_$repo" "$PY" audit-probe/diff/compare.py target/release/dartscope "$OUT/files_$repo.txt" "$OUT/ref_$repo.json" "$OUT"
  cp "$OUT/diff_report.txt" "$OUT/report_$repo.txt" 2>/dev/null
  emit "diff $repo" "$OUT/report_$repo.txt" --chunk 3900 --max 4
done
