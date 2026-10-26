# Daily upstream sync

The policy decided on 2026-10-13 is **“Ask me first” for any rendering change**. This supersedes the 2026-10-06 policy. The daily sync never records, commits or re-blesses a render reference.

`.github/workflows/upstream-sync.yml` runs at 09:23 UTC or on demand. The repository secret `UPSTREAM_SYNC_TOKEN` is configured. Do not dispatch the first sync until integration PR #179 has merged; the workflow also checks that prerequisite and stops without syncing until then.

1. An open sync PR labelled `needs-human` or `rendering-change` is preserved: later runs cannot overwrite its branch while a human reviews it.
2. Otherwise it merges upstream `main` into a fresh `upstream-sync` branch based on RapidRoom `main`. No new commits: stop. Conflicts: update one `upstream-sync` / `agent:codex` issue; an agent resolves them in a separate PR.
3. A clean candidate opens or updates a PR. It checks the **exact head and base** against every required hosted workflow: application build (`pr-ci.yml`), production CSP, Frontend Lint & Format, cargo fmt, cargo clippy, Headless CLI exit codes, Vitest, and regression. Applicable MCP-client and change-list checks are gated too. Every required job must succeed; skipped jobs are not success (apart from the change-list workflow's non-PR refresh job).
4. A missing, failed, cancelled or timed-out required check blocks merging. Only a diff that provably excludes a recognized workflow path filter permits an absent regression/MCP/change-list run. Unknown filter syntax fails closed. A successful regression run must have successful determinism **and comparison with the committed reference** steps; merely uploading a newly recorded reference is insufficient. The candidate cannot change that reference.
5. **Any render comparison or determinism failure:** keep the PR open and label it `needs-human` + `rendering-change`. Create/update an issue assigned to the maintainer, linking the `regression-diff` artifact with before/after panels and reports (14-day retention). The regression job renders the checked base for the before images, verifies it against its committed reference, and produces panels for differing cases. If evidence generation fails, its logs explain what is missing; the PR remains blocked until evidence and human approval are obtained.
6. **All required checks pass and rendering is unchanged:** recheck head and main immediately before `gh pr merge --match-head-commit`. A changed head blocks merging. An advanced base is merged into the candidate with a lease-protected push, then **all checks are required again for the new head/base**. Repeated base advances or conflicts leave the PR open. No `--auto` merge is queued. After merging, the actual merge commit's first parent is compared with the verified base. A mismatch opens a `needs-human` issue and dispatches the complete hosted check set on main.

Other failures leave the PR open labelled `agent:codex`, with the checked head/base and reason. The gate polls at five-minute intervals and has a five-hour total deadline, including base refreshes. This conditional daily-sync merge is the only automated exception in AGENTS.md; rendering changes still require explicit human approval through the normal review/reference process.

## Sync agent (2026-10-25)

The workflow merges only clean syncs whose renders are unchanged. Everything else goes to the sync agent, a Claude Code cloud routine that runs every four hours and acts as a maintainer account. Each run takes the next step for any open sync work and stops; the state lives in the PRs, issues, labels and comments.

- **Conflicts:** it resolves them on `sync/upstream-<date>`, keeping both behaviours as in the rules above, and opens a PR that closes the conflict issue.
- **Review:** a fresh subagent that has not seen the resolution reviews the merge against both parents. Merging needs a `Sync review: FINAL ACCEPT at <sha>` comment at the head, or at an earlier head followed only by reference or change-record commits.
- **Upstream render changes:** under the maintainer's standing approval (2026-10-23), it records a new `ci-reference.json` with `regression.yml` (`record=true`) and commits it. This applies only when upstream commits in the sync change rendering code, the resolution keeps upstream's render logic as is, and the diffs match that change. Any other render change, or any doubt, keeps the PR held: `needs-human` + `rendering-change` plus a `Sync agent: holding for the maintainer` comment that mentions the maintainer.
- **Merge:** only when every required hosted check passed at the exact head, the review accepted it, the PR is not held, and `main` has not moved since the checks ran. It merges with the expected head sha, then verifies the merge commit's first parent, as the workflow does.
- **Never:** it never pushes to `main`, re-blesses a RapidRoom-caused render change, or merges with a check missing.

The routine's prompt is kept with the maintainer's project notes. Pause it at claude.ai/code/routines.

## Remaining base race

Main deliberately keeps the [non-strict status policy](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) to avoid forcing all parallel PRs to refresh and repeat long CI after every merge. GitHub binds the merge to the expected head, while our last base read and the merge are separate requests. A seconds-wide base race remains; the post-merge first-parent check detects it and escalates to human review plus full hosted checks on main. The workflow does not silently claim that combination was verified, roll it back, or re-bless a reference.

## Token and maintenance

Use a fine-grained token limited to **RapidRoom/rapidroom**, owned by a maintainer, with read/write **Contents, Pull requests, Issues, Actions and Workflows**, and an expiry of at most 366 days. Store it as `UPSTREAM_SYNC_TOKEN`; renew before expiry. A missing/expired token fails closed. The built-in `GITHUB_TOKEN` cannot push upstream workflow commits or trigger the PR checks.

When required workflows, job names or path filters change, update `rapidroom/upstream-sync/gate.sh` and its mocked tests together. Local validation: `bash rapidroom/upstream-sync/test-gate.sh`, Python panel tests, and actionlint. These do not invoke GitHub, render photos, merge a PR or dispatch the live daily sync.
