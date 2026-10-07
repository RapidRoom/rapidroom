# Daily upstream sync

RapidRoom follows upstream [RapidRAW](https://github.com/CyberTimon/RapidRAW) `main` every day, so merges stay small. `.github/workflows/upstream-sync.yml` runs at 09:23 UTC (and on demand from the Actions tab):

1. It merges upstream `main` into a fresh `upstream-sync` branch cut from RapidRoom `main`. Nothing new: it stops.
2. **Conflict:** it opens (or updates) one issue, "Upstream sync needs conflict resolution", labelled `upstream-sync` and `agent:codex`, listing the conflicting files and the upstream commits. An agent resolves the conflicts on its own branch and opens a PR (AGENTS.md rules: keep both behaviours, say what was done). After that lands, the daily run merges by itself again.
3. **Clean merge:** it opens (or updates) the "Sync upstream RapidRAW …" PR and waits for the usual checks: Lint (cargo fmt, cargo clippy, frontend lint), Vitest, and the pixel-exact regression check when `src-tauri/` changed.
4. **Renders changed** (deterministically): it records a new `ci-reference.json` on the branch, reruns the required checks, merges, and opens a `needs-human` + `rendering-change` issue linking the run with the differing renders, so a person looks later and re-blesses the private 60-render baseline.
5. **Everything green:** it merges the PR. **Something red** (for example new clippy warnings from upstream): it leaves the PR open, labels it `agent:codex` and comments which check failed.

This is the one automated merge into `main`. The maintainers decided on 2026-10-06 that upstream is trusted enough to merge without a person in the loop, with a later double-check.

## Setup

The workflow needs the repository secret **`UPSTREAM_SYNC_TOKEN`**. The built-in `GITHUB_TOKEN` can't push upstream commits that touch `.github/workflows/`, and PRs it opens don't start other workflows.

Create a fine-grained personal access token as a maintainer (GitHub → Settings → Developer settings → Fine-grained tokens):

- Resource owner: **RapidRoom**; repository access: only **RapidRoom/rapidroom**; expiry: at most 366 days (organisation rule).
- Repository permissions, read and write: **Contents, Pull requests, Issues, Actions, Workflows** (Metadata read is automatic).

Store it with `gh secret set UPSTREAM_SYNC_TOKEN -R RapidRoom/rapidroom` (paste when asked) or in the repository's Settings → Secrets and variables → Actions. Renew it before it expires; a missing or expired token makes the run fail at its first step.
