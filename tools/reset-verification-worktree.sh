#!/usr/bin/env bash
set -euo pipefail

remote="${1:-origin}"
branch="${2:-main}"

git fetch "$remote" "$branch"
git reset --hard "$remote/$branch"
# `reset --hard` leaves untracked generated payload files behind. Failure-report paths must clean them
# before selectively removing tracked staging files and writing the report.
git clean -fdx
