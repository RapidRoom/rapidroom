#!/usr/bin/env bash
set -euo pipefail

conflicts_file=${1:?Usage: resolve-generated.sh <conflicts-file> <toolchain> <manifest>}
toolchain=${2:?Usage: resolve-generated.sh <conflicts-file> <toolchain> <manifest>}
manifest=${3:?Usage: resolve-generated.sh <conflicts-file> <toolchain> <manifest>}
mapfile -t conflicts < <(sed '/^$/d' "$conflicts_file")
if [ "${#conflicts[@]}" -ne 1 ] || [ "${conflicts[0]}" != src-tauri/Cargo.lock ]; then
  echo "Mechanical resolution is limited to a lone src-tauri/Cargo.lock conflict."
  exit 1
fi

git checkout --theirs -- src-tauri/Cargo.lock
(
  cd "$RUNNER_TEMP"
  cargo "+$toolchain" update -w --manifest-path "$manifest"
)
git add -- src-tauri/Cargo.lock
remaining=$(git diff --name-only --diff-filter=U)
if [ -n "$remaining" ]; then
  echo "Cargo.lock resolution left unmerged files:"
  echo "$remaining"
  exit 1
fi

git commit --no-edit --cleanup=strip
