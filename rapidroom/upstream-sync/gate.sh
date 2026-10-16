#!/usr/bin/env bash
# The daily sync may merge only the checked head and base, without changing references.
set -euo pipefail

poll_pause() { sleep "${SYNC_POLL_SECONDS:-300}"; }
current_base() { gh api "repos/$GH_REPO/git/ref/heads/main" --jq .object.sha; }
current_head() { gh api "repos/$GH_REPO/pulls/$PR" --jq .head.sha; }
human_requested() { gh api "repos/$GH_REPO/pulls/$PR" --jq '.labels | any(.name == "needs-human" or .name == "rendering-change")'; }

filter_excludes_diff() {
  local workflow=$1 base=$2 head=$3
  # Recognize only the currently reviewed filters. Unknown syntax fails closed.
  python3 - "$workflow" "$base" "$head" <<'PY'
import subprocess, sys
workflow, base, head = sys.argv[1:]
filters = {
    "regression.yml": ["src-tauri/**", "rapidroom/regression/**", ".github/workflows/regression.yml"],
    "mcp-client.yml": ["rapidroom/mcp-client/**", ".github/workflows/mcp-client.yml"],
    "rapidroom-status.yml": ["rapidroom/**", ".github/README.md", "CHANGES.md"],
}
patterns = filters[workflow]
text = subprocess.check_output(["git", "show", f"{base}:.github/workflows/{workflow}"], text=True)
lines = text.splitlines()
try:
    start = lines.index("  pull_request:")
except ValueError:
    sys.exit(1)
block = []
for line in lines[start + 1:]:
    if line and not line.startswith("    "):
        break
    if line.strip() and not line.lstrip().startswith("#"):
        block.append(line)
expected = ["    paths:", *[f"      - '{pattern}'" for pattern in patterns]]
if block != expected:
    sys.exit(1)
paths = subprocess.check_output(["git", "diff", "--no-renames", "--name-only", "-z", base, head]).decode().split("\0")
def matches(path, pattern):
    return path.startswith(pattern[:-2]) if pattern.endswith("**") else path == pattern
sys.exit(1 if any(matches(path, pattern) for path in paths if path for pattern in patterns) else 0)
PY
}

jobs_pass() {
  local workflow=$1 jobs=$2
  python3 - "$workflow" "$jobs" <<'PY'
import json, sys
workflow, path = sys.argv[1:]
jobs = json.load(open(path))["jobs"]
required = {
    "lint.yml": {"Frontend Lint & Format", "cargo fmt", "cargo clippy", "Headless CLI exit codes"},
    "csp.yml": {"csp"},
    "frontend-test.yml": {"Vitest"},
    "regression.yml": {"regression"},
    "mcp-client.yml": {"client"},
    "rapidroom-status.yml": {"check"},
}
names = {job["name"] for job in jobs}
if workflow == "pr-ci.yml":
    if not any(name.startswith("build") and name.endswith(" / build") for name in names):
        sys.exit(1)
elif not required[workflow].issubset(names):
    sys.exit(1)
for job in jobs:
    if workflow == "rapidroom-status.yml" and job["name"] == "refresh" and job["conclusion"] == "skipped":
        continue
    if job["conclusion"] != "success":
        sys.exit(1)
if workflow == "regression.yml":
    steps = {step["name"]: step["conclusion"] for job in jobs for step in job["steps"]}
    for name in ["Determinism (run 1 vs run 2)", "Compare with ci-reference.json"]:
        if steps.get(name) != "success":
            sys.exit(1)
PY
}

render_changed() {
  jq -e '[.jobs[].steps[] | select(.name == "Compare with ci-reference.json" or .name == "Determinism (run 1 vs run 2)") | .conclusion] | any(. == "failure")' "$1" >/dev/null
}

report_human() {
  local reason=$1 run_id=${2:-} title issue
  gh pr edit "$PR" --add-label needs-human --add-label rendering-change
  title="Upstream sync needs render approval (${HEAD:0:8})"
  {
    echo "Ask the maintainer first: PR #$PR remains open. **Nothing was merged or re-blessed.**"
    echo
    echo "$reason"
    echo
    echo "Checked head: \`$HEAD\`; base: \`$BASE\`."
    if [[ -n "$run_id" ]]; then
      echo "Before/after panels and comparison reports: artifact \`regression-diff\` on https://github.com/$GH_REPO/actions/runs/$run_id (14-day retention)."
      echo "If panel generation failed, its logs are in that run; obtain the missing evidence before approval."
    fi
    echo "A human must review the evidence before deciding whether to accept changed rendering. This workflow cannot update the reference."
  } > "$RUNNER_TEMP/upstream-sync-human.md"
  issue=$(gh issue list --state open --label upstream-sync --search "in:title \"$title\"" --json number --jq '.[0].number // empty')
  if [[ -n "$issue" ]]; then
    gh issue edit "$issue" --body-file "$RUNNER_TEMP/upstream-sync-human.md"
  else
    gh issue create --title "$title" --label upstream-sync --label needs-human --label rendering-change --assignee yojen7 --body-file "$RUNNER_TEMP/upstream-sync-human.md"
  fi
}

report_failed() {
  gh pr edit "$PR" --add-label agent:codex
  gh pr comment "$PR" --body "Not merged: $1. Checked head \`$HEAD\`, base \`$BASE\`. No reference was updated. Run: https://github.com/$GH_REPO/actions/runs/$GITHUB_RUN_ID"
}

wait_for_checks() {
  local runs workflow row id status conclusion jobs pending failed
  local -a required=(pr-ci.yml csp.yml lint.yml frontend-test.yml)
  for workflow in regression.yml mcp-client.yml rapidroom-status.yml; do
    if ! filter_excludes_diff "$workflow" "$BASE" "$HEAD"; then
      required+=("$workflow")
    else
      echo "$workflow: actual diff provably excludes the reviewed path filter"
    fi
  done
  while (( SECONDS < DEADLINE )); do
    [[ $(current_head) == "$HEAD" ]] || { report_failed 'the PR head changed'; return 1; }
    [[ $(current_base) == "$BASE" ]] || return 3
    runs=$(gh api "repos/$GH_REPO/actions/runs?head_sha=$HEAD&event=pull_request&per_page=100")
    pending=false
    failed=""
    for workflow in "${required[@]}"; do
      row=$(jq -r --arg path ".github/workflows/$workflow" --arg sha "$HEAD" --arg base "$BASE" --argjson pr "$PR" '[.workflow_runs[] | select(.path == $path and .head_sha == $sha and any(.pull_requests[]; .number == $pr and .head.sha == $sha and .base.sha == $base))] | sort_by(.id) | last | if . == null then "none pending none" else "\(.id) \(.status) \(.conclusion // "none")" end' <<< "$runs")
      read -r id status conclusion <<< "$row"
      if [[ "$status" != completed ]]; then
        pending=true
        continue
      fi
      jobs="$RUNNER_TEMP/upstream-sync-$id-jobs.json"
      gh api --paginate "repos/$GH_REPO/actions/runs/$id/jobs?per_page=100" --slurp | jq '{jobs: [.[].jobs[]]}' > "$jobs"
      if [[ "$workflow" == regression.yml ]] && render_changed "$jobs"; then
        report_human 'The candidate renders differ from the committed reference or are nondeterministic.' "$id"
        return 2
      fi
      if [[ "$conclusion" != success ]] || ! jobs_pass "$workflow" "$jobs"; then
        failed="$workflow did not pass every required job/step ($conclusion)"
        continue
      fi
      echo "$workflow: $id success on $HEAD"
    done
    if [[ "$pending" != true ]]; then
      [[ -z "$failed" ]] || { report_failed "$failed"; return 1; }
      return 0
    fi
    poll_pause
  done
  report_failed 'required checks were missing or timed out (no unproven regression skip)'
  return 1
}

refresh_base() {
  [[ $(current_head) == "$HEAD" ]] || { report_failed 'the PR head changed before refreshing main'; return 1; }
  git fetch --no-tags origin main
  git checkout -B "$BRANCH" "$HEAD"
  if ! git merge --no-ff --no-edit origin/main; then
    git merge --abort
    report_failed 'main advanced and the refresh conflicts'
    return 1
  fi
  if [[ $(git rev-parse HEAD) == "$HEAD" ]]; then
    git commit --allow-empty -m "Recheck upstream sync against advanced main"
  fi
  git push "--force-with-lease=refs/heads/$BRANCH:$HEAD" origin "HEAD:refs/heads/$BRANCH"
  BASE=$(git rev-parse origin/main)
  HEAD=$(git rev-parse HEAD)
}

verify_merged_base() {
  local commit parent
  commit=$(gh api "repos/$GH_REPO/pulls/$PR" --jq .merge_commit_sha) || commit=unknown
  parent=$(gh api "repos/$GH_REPO/git/commits/$commit" --jq '.parents[0].sha') || parent=unknown
  [[ "$parent" != "$BASE" ]] || return 0
  {
    echo "PR #$PR merged as \`$commit\`, but its first parent \`$parent\` differs from verified base \`$BASE\`."
    echo "The final read/merge boundary was not verified (main advanced or the parent lookup failed). The checked head was \`$HEAD\`."
    echo "Human review is required; the full hosted check set is being dispatched on main. No reference was changed."
    echo "Run: https://github.com/$GH_REPO/actions/runs/$GITHUB_RUN_ID"
  } > "$RUNNER_TEMP/upstream-sync-base-race.md"
  gh issue create --title "Upstream sync base changed during merge (${commit:0:8})" \
    --label upstream-sync --label needs-human --assignee yojen7 --body-file "$RUNNER_TEMP/upstream-sync-base-race.md" || true
  local workflow
  for workflow in ci.yml csp.yml lint.yml frontend-test.yml regression.yml mcp-client.yml rapidroom-status.yml; do
    gh workflow run "$workflow" --ref main || echo "::error::Could not dispatch $workflow; human follow-up required."
  done
  return 1
}

gate_and_merge() {
  PR=$1 BASE=$2 HEAD=$3
  DEADLINE=$((SECONDS + ${SYNC_WAIT_SECONDS:-18000}))
  local attempt result human
  for attempt in 1 2 3; do
    human=$(human_requested) || return 1
    [[ "$human" == false ]] || { report_human 'Human review was requested for this candidate.'; return 2; }
    git merge-base --is-ancestor "$BASE" "$HEAD" || { report_failed 'the captured base is not in the candidate'; return 1; }
    if ! git cat-file -e "$BASE:rapidroom/regression/ci-reference.json" ||
       ! git diff --quiet "$BASE" "$HEAD" -- rapidroom/regression/ci-reference.json; then
      report_human 'The committed render reference is missing or changed on the candidate; automatic approval is forbidden.'
      return 2
    fi
    result=0
    wait_for_checks || result=$?
    if [[ "$result" == 3 ]]; then
      refresh_base || return 1
      continue
    fi
    [[ "$result" == 0 ]] || return "$result"
    [[ $(current_head) == "$HEAD" ]] || { report_failed 'the PR head changed before merge'; return 1; }
    human=$(human_requested) || return 1
    [[ "$human" == false ]] || { report_human 'Human review was requested before merge.'; return 2; }
    if [[ $(current_base) != "$BASE" ]]; then
      refresh_base || return 1
      continue
    fi
    gh pr merge "$PR" --merge --delete-branch --match-head-commit "$HEAD"
    verify_merged_base
    return $?
  done
  report_failed 'main kept advancing; no candidate with the current base was fully checked'
  return 1
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  gate_and_merge "$@"
fi
