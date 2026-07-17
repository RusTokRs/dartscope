#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "CARGO_REGISTRY_TOKEN is required" >&2
  exit 1
fi

version="$(
  python3 - <<'PY'
from pathlib import Path
import tomllib

manifest = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))
print(manifest["workspace"]["package"]["version"])
PY
)"

expected_tag="v${version}"
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  if [[ "${GITHUB_REF_TYPE:-}" != "tag" || "${GITHUB_REF_NAME:-}" != "${expected_tag}" ]]; then
    echo "publishing requires workflow dispatch from tag ${expected_tag}" >&2
    exit 1
  fi
fi

crate_visible() {
  cargo info "$1@${version}" --registry crates-io >/dev/null 2>&1
}

while IFS= read -r crate; do
  [[ -z "${crate}" || "${crate}" == \#* ]] && continue

  if crate_visible "${crate}"; then
    echo "${crate} ${version} is already visible; skipping"
    continue
  fi

  cargo publish -p "${crate}" --locked --registry crates-io

  visible=false
  for _attempt in $(seq 1 60); do
    if crate_visible "${crate}"; then
      visible=true
      break
    fi
    sleep 10
  done
  if [[ "${visible}" != "true" ]]; then
    echo "${crate} ${version} did not become visible in the registry" >&2
    exit 1
  fi
done < tools/release-crates.txt
