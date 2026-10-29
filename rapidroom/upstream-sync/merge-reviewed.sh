#!/usr/bin/env bash
# Merge an agent-resolved sync PR (branch sync/upstream-*) once the sync agent's
# review covers its exact head and every required check passed there. The agent
# cannot call the merge API itself; see rapidroom/UPSTREAM-SYNC.md ("Sync agent").
set -euo pipefail

# The daily gate's filter, job and render checks are reused so both merge paths apply the same rules.
source "$(dirname "${BASH_SOURCE[0]}")/gate.sh"

REVIEWER=${SYNC_REVIEWER:-yojen7}
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
    [[ -n "${p1:-}" ]] || return 1
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

# Prints why the head's checks don't allow a merge; returns 0 when they do. Like gate.sh, each
# required workflow needs its latest pull_request run for this PR, head and base, every required
# job must succeed, and regression must pass its determinism and reference-comparison steps.
checks_pass() {
  local head=$1 base=$2 runs workflow row id status conclusion jobs
  local -a required=(pr-ci.yml csp.yml lint.yml frontend-test.yml)
  for workflow in regression.yml mcp-client.yml rapidroom-status.yml; do
    filter_excludes_diff "$workflow" "$base" "$head" || required+=("$workflow")
  done
  git cat-file -e "$head:rapidroom/regression/ci-reference.json" || { echo "the head has no ci-reference.json"; return 1; }
  runs=$(gh api "repos/$GH_REPO/actions/runs?head_sha=$head&event=pull_request&per_page=100")
  for workflow in "${required[@]}"; do
    row=$(jq -r --arg path ".github/workflows/$workflow" --arg sha "$head" --arg base "$base" --argjson pr "$PR" '[.workflow_runs[] | select(.path == $path and .head_sha == $sha and any(.pull_requests[]; .number == $pr and .head.sha == $sha and .base.sha == $base))] | sort_by(.id) | last | if . == null then "none none none" else "\(.id) \(.status) \(.conclusion // "none")" end' <<<"$runs")
    read -r id status conclusion <<<"$row"
    [[ "$id" != none ]] || { echo "$workflow has no pull_request run for this head and base"; return 1; }
    [[ "$status" == completed ]] || { echo "$workflow is $status"; return 1; }
    jobs="$RUNNER_TEMP/sync-merge-$id-jobs.json"
    gh api --paginate "repos/$GH_REPO/actions/runs/$id/jobs?per_page=100" --slurp | jq '{jobs: [.[].jobs[]]}' >"$jobs"
    if [[ "$workflow" == regression.yml ]] && render_changed "$jobs"; then
      echo "renders differ from the reference (the sync agent handles render changes)"
      return 1
    fi
    [[ "$conclusion" == success ]] && jobs_pass "$workflow" "$jobs" || { echo "$workflow did not pass every required job/step ($conclusion)"; return 1; }
  done
}

verify_first_parent() {
  local commit parent
  commit=$(gh api "repos/$GH_REPO/pulls/$PR" --jq .merge_commit_sha) || commit=unknown
  parent=$(gh api "repos/$GH_REPO/git/commits/$commit" --jq '.parents[0].sha') || parent=unknown
  [[ "$parent" != "$MAIN" ]] || return 0
  gh issue create --title "Upstream sync base changed during merge (${commit:0:8})" --label upstream-sync --label needs-human \
    --assignee "$REVIEWER" --body "PR #$PR merged as \`$commit\`, but its first parent \`$parent\` differs from the verified base \`$MAIN\`. Checked head: \`$HEAD\`. The full hosted check set is being dispatched on main. Run: https://github.com/$GH_REPO/actions/runs/${GITHUB_RUN_ID:-}" || true
  local workflow
  for workflow in ci.yml csp.yml lint.yml frontend-test.yml regression.yml mcp-client.yml rapidroom-status.yml; do
    gh workflow run "$workflow" --ref main || echo "::error::Could not dispatch $workflow; human follow-up required."
  done
  return 1
}

merge_reviewed() {
  PR=$1
  local state ref repo target draft labels reviewed why
  read -r state ref repo target draft HEAD labels <<<"$(gh api "repos/$GH_REPO/pulls/$PR" \
    --jq '"\(.state) \(.head.ref) \(.head.repo.full_name) \(.base.ref) \(.draft) \(.head.sha) ,\([.labels[].name] | join(",")),"')"
  [[ "$state" == open ]] || skip "it is $state"
  [[ "$ref" == sync/upstream-* ]] || skip "branch $ref is not an agent-resolved sync branch"
  [[ "$repo" == "$GH_REPO" ]] || skip "the head is in $repo"
  [[ "$target" == main ]] || skip "it targets $target, not main"
  [[ "$labels" != *,needs-human,* && "$labels" != *,rendering-change,* ]] || skip "it is held (needs-human or rendering-change)"
  reviewed=$(gh api --paginate --slurp "repos/$GH_REPO/issues/$PR/comments?per_page=100" | jq -r --arg r "$REVIEWER" \
    '[.[][] | select(.user.login == $r and (.body | startswith("Sync review:")))] | last // empty
     | .body | capture("^Sync review: FINAL ACCEPT at (?<sha>[0-9a-f]{40})")?.sha // empty')
  [[ -n "$reviewed" ]] || skip "the latest 'Sync review:' comment from $REVIEWER is not a FINAL ACCEPT"

  git fetch --no-tags -q origin main "+refs/heads/$ref:refs/remotes/origin/$ref"
  MAIN=$(git rev-parse origin/main)
  [[ "$(git rev-parse "origin/$ref")" == "$HEAD" ]] || skip "the branch moved while it was being read"
  git merge-base --is-ancestor "$MAIN" "$HEAD" || skip "main ${MAIN:0:8} is not in head ${HEAD:0:8} yet (the sync agent merges it in)"
  review_covers "$reviewed" "$HEAD" "$MAIN" || skip "the review at ${reviewed:0:8} does not cover head ${HEAD:0:8}"
  why=$(checks_pass "$HEAD" "$MAIN") || skip "checks at ${HEAD:0:8}: $why"

  [[ "$(gh api "repos/$GH_REPO/pulls/$PR" --jq .head.sha)" == "$HEAD" ]] || skip "the head changed"
  [[ "$(gh api "repos/$GH_REPO/git/ref/heads/main" --jq .object.sha)" == "$MAIN" ]] || skip "main moved"
  [[ "$(gh api "repos/$GH_REPO/pulls/$PR" --jq '[.labels[].name] | any(. == "needs-human" or . == "rendering-change")')" == false ]] || skip "it became held"
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
