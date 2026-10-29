#!/usr/bin/env bash
set -euo pipefail

conflicts_file=${1:?Usage: resolve-generated.sh <conflicts-file>}
mapfile -t conflicts < <(sed '/^$/d' "$conflicts_file")
if [ "${#conflicts[@]}" -eq 0 ]; then
  echo "No conflicts listed; nothing to resolve."
  exit 1
fi

resolve_lock=false
resolve_schema=false
resolve_reference=false
for file in "${conflicts[@]}"; do
  case "$file" in
    src-tauri/Cargo.lock) resolve_lock=true ;;
    rapidroom/adjustment-schema.json) resolve_schema=true ;;
    rapidroom/plugin/skills/rapidroom/references/adjustments.md) resolve_reference=true ;;
    *)
      echo "Non-generated conflict remains: $file"
      exit 1
      ;;
  esac
done

if [ "$resolve_lock" = true ]; then
  git checkout --ours -- src-tauri/Cargo.lock
  if ! cargo update -w --manifest-path src-tauri/Cargo.toml --offline; then
    echo "Offline Cargo resolution failed; retrying with the registry available."
    cargo update -w --manifest-path src-tauri/Cargo.toml
  fi
fi

if [ "$resolve_schema" = true ] || [ "$resolve_reference" = true ]; then
  npm ci
fi
if [ "$resolve_schema" = true ]; then
  node rapidroom/generate-adjustment-schema.mjs
fi
if [ "$resolve_reference" = true ]; then
  npx vitest run src/utils/adjustmentReference.test.ts -u
fi

git add -- "${conflicts[@]}"
remaining=$(git diff --name-only --diff-filter=U)
if [ -n "$remaining" ]; then
  echo "Generated conflict resolution left unmerged files:"
  echo "$remaining"
  exit 1
fi

git commit --no-edit
