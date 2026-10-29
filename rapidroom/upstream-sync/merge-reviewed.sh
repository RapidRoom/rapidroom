#!/usr/bin/env bash
# Merge an agent-resolved sync PR (branch sync/upstream-*) once the sync agent's
# review covers its exact head and every required check passed there. The agent
# cannot call the merge API itself; see rapidroom/UPSTREAM-SYNC.md ("Sync agent").
set -euo pipefail

REVIEWER=${SYNC_REVIEWER:-yojen7}
REQUIRED_CHECKS=("csp" "Frontend Lint & Format" "cargo fmt" "cargo clippy" "Headless CLI exit codes" "Vitest" "regression" "check")
STANDING='Covered by the standing maintainer approval for upstream render changes (2026-10-23).'

skip() {
  echo "PR #$PR not merged: $*" | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit 0
}

# Commits after the reviewed head may only be clean merges of main, change records,
# or reference re-records under the standing approval for upstream render changes.
review_covers() {
  local reviewed=$1 head=$2 main=$3 c p1 p2 extra tree files
  git merge-base --is-ancestor "$reviewed" "$head" || return 1
  for c in $(git rev-list "$reviewed..$head" --not "$main"); do
    read -r _ p1 p2 extra <<<"$(git rev-list --parents -n1 "$c")"
    if [[ -n "${p2:-}" ]]; then
      [[ -z "${extra:-}" ]] || return 1
      git merge-base --is-ancestor "$p2" "$main" || return 1
      tree=$(git merge-tree --write-tree "$p1" "$p2") || return 1
      [[ "${tree%%$'\n'*}" == "$(git rev-parse "$c^{tree}")" ]] || return 1
      continue
    fi
    files=$(git diff-tree --no-commit-id --name-only -r "$c")
    [[ -n "$files" ]] || continue
    if grep -qvE '^rapidroom/changes/[^/]+\.json$' <<<"$files"; then
      ! grep -qvE '^(rapidroom/changes/[^/]+\.json|rapidroom/regression/ci-reference\.json)$' <<<"$files" || return 1
      git log -1 --format=%B "$c" | grep -qF "$STANDING" || return 1
    fi
  done
}

# Prints why the head's checks don't allow a merge; returns 0 when they do.
checks_pass() {
  gh api --paginate "repos/$GH_REPO/commits/$1/check-runs?per_page=100" \
    --jq '.check_runs[] | [.name, .status, (.conclusion // "")] | @tsv' >"$RUNNER_TEMP/sync-checks.tsv"
  python3 - "$RUNNER_TEMP/sync-checks.tsv" "${REQUIRED_CHECKS[@]}" <<'PY'
import sys
rows = [line.rstrip("\n").split("\t") for line in open(sys.argv[1]) if line.strip()]
names = {row[0] for row in rows}
missing = [name for name in sys.argv[2:] if name not in names]
if not any(name.startswith("build") and name.endswith(" / build") for name in names):
    missing.append("build (...) / build")
bad = [f"{name} {status}/{conclusion or '-'}" for name, status, conclusion in rows
       if status != "completed" or (conclusion != "success" and not (name == "refresh" and conclusion == "skipped"))]
reasons = (["missing " + ", ".join(missing)] if missing else []) + (["not passing " + ", ".join(bad)] if bad else [])
if reasons:
    print("; ".join(reasons))
    sys.exit(1)
PY
}

verify_first_parent() {
  local commit parent
  commit=$(gh api "repos/$GH_REPO/pulls/$PR" --jq .merge_commit_sha) || commit=unknown
  parent=$(gh api "repos/$GH_REPO/git/commits/$commit" --jq '.parents[0].sha') || parent=unknown
  [[ "$parent" != "$MAIN" ]] || return 0
  gh issue create --title "Upstream sync base changed during merge (${commit:0:8})" --label upstream-sync --label needs-human \
    --assignee "$REVIEWER" --body "PR #$PR merged as \`$commit\`, but its first parent \`$parent\` differs from the verified base \`$MAIN\`. Checked head: \`$HEAD\`. The full hosted check set is being dispatched on main. Run: https://github.com/$GH_REPO/actions/runs/${GITHUB_RUN_ID:-}" || true
  local workflow
  for workflow in pr-ci.yml csp.yml lint.yml frontend-test.yml regression.yml rapidroom-status.yml; do
    gh workflow run "$workflow" --ref main || echo "::error::Could not dispatch $workflow; human follow-up required."
  done
  return 1
}

merge_reviewed() {
  PR=$1
  local state ref repo draft labels reviewed why
  read -r state ref repo draft HEAD labels <<<"$(gh api "repos/$GH_REPO/pulls/$PR" \
    --jq '"\(.state) \(.head.ref) \(.head.repo.full_name) \(.draft) \(.head.sha) ,\([.labels[].name] | join(",")),"')"
  [[ "$state" == open ]] || skip "it is $state"
  [[ "$ref" == sync/upstream-* ]] || skip "branch $ref is not an agent-resolved sync branch"
  [[ "$repo" == "$GH_REPO" ]] || skip "the head is in $repo"
  [[ "$labels" != *,needs-human,* ]] || skip "it is labelled needs-human"
  reviewed=$(gh api --paginate --slurp "repos/$GH_REPO/issues/$PR/comments?per_page=100" | jq -r --arg r "$REVIEWER" \
    '[.[][] | select(.user.login == $r and (.body | test("^Sync review: FINAL ACCEPT at [0-9a-f]{40}")))] | last // empty
     | .body | capture("^Sync review: FINAL ACCEPT at (?<sha>[0-9a-f]{40})").sha')
  [[ -n "$reviewed" ]] || skip "no 'Sync review: FINAL ACCEPT' comment from $REVIEWER"

  git fetch --no-tags -q origin main "+refs/heads/$ref:refs/remotes/origin/$ref"
  MAIN=$(git rev-parse origin/main)
  [[ "$(git rev-parse "origin/$ref")" == "$HEAD" ]] || skip "the branch moved while it was being read"
  git merge-base --is-ancestor "$MAIN" "$HEAD" || skip "main ${MAIN:0:8} is not in head ${HEAD:0:8} yet (the sync agent merges it in)"
  review_covers "$reviewed" "$HEAD" "$MAIN" || skip "the review at ${reviewed:0:8} does not cover head ${HEAD:0:8}"
  why=$(checks_pass "$HEAD") || skip "checks at ${HEAD:0:8}: $why"

  [[ "$(gh api "repos/$GH_REPO/pulls/$PR" --jq .head.sha)" == "$HEAD" ]] || skip "the head changed"
  [[ "$(gh api "repos/$GH_REPO/git/ref/heads/main" --jq .object.sha)" == "$MAIN" ]] || skip "main moved"
  [[ "$(gh api "repos/$GH_REPO/pulls/$PR" --jq '[.labels[].name] | any(. == "needs-human")')" == false ]] || skip "it is labelled needs-human"
  [[ "$draft" == false ]] || gh pr ready "$PR"
  gh pr merge "$PR" --merge --match-head-commit "$HEAD"
  verify_first_parent || exit 1
  gh api -X DELETE "repos/$GH_REPO/git/refs/heads/$ref" >/dev/null || echo "::warning::Could not delete $ref."
  gh pr comment "$PR" --body "Merged by the upstream-sync merge workflow: \`Sync review: FINAL ACCEPT\` at \`${reviewed:0:8}\` covers head \`${HEAD:0:8}\`, every required check passed there, and the merge's first parent is the verified main \`${MAIN:0:8}\`. Run: https://github.com/$GH_REPO/actions/runs/${GITHUB_RUN_ID:-}"
  echo "PR #$PR merged at ${HEAD:0:8}." | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  merge_reviewed "$@"
fi
