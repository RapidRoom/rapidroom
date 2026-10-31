# Working on RapidRoom (for AI agents and humans)

RapidRoom is a community build of [RapidRAW](https://github.com/CyberTimon/RapidRAW): upstream `main` plus fixes and features harvested from forks, each credited. Read [.github/CONTRIBUTING.md](.github/CONTRIBUTING.md) and [.github/GOVERNANCE.md](.github/GOVERNANCE.md) first. This file adds the working rules for agents.

## Picking work

- Work comes from issues. Agent queues are the labels `agent:codex` and `agent:claude-cloud`. Take only issues with your label, and comment on the issue when you start, so two agents never work on the same thing.
- `needs-human` means a person has to test, judge or decide something. Do the code, then say exactly what needs checking. Don't mark the issue done.
- One issue per branch and per PR. Branch names: `codex/<issue>-<slug>` or `claude/<issue>-<slug>`. Cloud sessions that are assigned a branch name may keep it. Either way, put the issue number in the PR title and write `Closes #<issue>` in the body.
- PRs go to `RapidRoom/rapidroom` `main`. A maintainer reviews and merges them. Never push to `main` directly. The daily upstream sync is the only automated exception: it may merge only after every required hosted check passes for the exact candidate head/base and renders match the committed reference (a proven path-filter exclusion is the only allowed missing regression run). Any render change keeps the PR open labelled `needs-human` + `rendering-change`, with before/after evidence and a maintainer issue; it never re-blesses or merges changed renders. A changed head blocks merging; an advanced base requires a refresh and new checks. The residual read/merge base race is checked afterward: an unexpected merge first parent opens a human-review issue and dispatches the full hosted check set on main. See [rapidroom/UPSTREAM-SYNC.md](rapidroom/UPSTREAM-SYNC.md), policy updated 2026-10-13. Conflict issues, failed sync PRs and upstream render changes go to the **sync agent**, a scheduled Claude Code routine. It resolves conflicts and posts `Sync sign-off requested at <sha>`; it does not post `Sync review:` comments or merge. A separate read-only reviewer posts `Sync review: FINAL ACCEPT at <sha>` or `Sync review: not accepted at <sha>`. The upstream-sync merge workflow merges an accepted, fully checked head (exact-head checks, and the maintainer's standing approval for upstream-only render changes from 2026-10-23). See the sync agent and reviewer section of that file.

## Cloud sessions: picking the next issue

When a session is started with "take the next cloud issue":

1. List open issues labelled `agent:claude-cloud`, oldest first: `gh issue list -R RapidRoom/rapidroom -l agent:claude-cloud --state open`, or the GitHub web UI.
2. Skip an issue if it has a "working on this" comment from another session in the last 24 hours, or an open PR that closes it. Also skip it if its body or an **Agent brief** comment says to wait for another issue that's still open.
3. Comment "Claude Code cloud session: working on this", then follow the issue body and any **Agent brief** comment. The brief wins if they disagree.
4. One issue per session. Open a PR into `main` titled `#<issue> <summary>` that closes the issue. Then stop.
5. **Keep your PR mergeable.** Other sessions merge into `main` while yours is open. When your PR conflicts, merge `main` into your branch (don't force-push over reviewed commits). Keep every independent `rapidroom/changes/*.json` record and every `CREDITS.md` row. Do not regenerate `CHANGES.md` or the README status block in PRs. Run `node rapidroom/status.mjs --check`, rerun applicable checks, and push. Code conflicts with another PR: keep both behaviours, and say in a PR comment what you did.

## Harvesting from forks

- **Keep authorship:** `git cherry-pick -x <sha>`. If you have to adapt a change, keep the original author on the commit (`--author`), or add a `Co-authored-by:` trailer plus a `(from <fork>@<sha>)` line.
- Drop changes the harvest doesn't need (the fork's own changelog, tests for code RapidRoom doesn't have) and say so in the commit message.
- Add the source to `CREDITS.md`.

## Locale ownership

- Keep `src/i18n/locales/<lang>.json` byte-identical to upstream. During upstream syncs, take these files from upstream as is.
- Add RapidRoom keys and overrides (including branding and completed plural forms) only in `src/i18n/locales/rapidroom/<lang>.json`. The overlay deep-merges over upstream; overlay values win.
- `npm run i18n:extract` compares complete merged catalogs in temporary files and writes only the RapidRoom differences. Run `npm run i18n:check` and `npm run i18n:runtime-check` after changing strings. Do not format or hand-edit upstream catalogs.

## Every PR

- **Lockfile:** if you change dependencies, regenerate `package-lock.json` with **npm 11 or newer**. npm 10 drops the existing `libc` fields from the lockfile.

- **Changelog:** add one record at `rapidroom/changes/<issue>-<slug>.json`, following [the record guide](rapidroom/changes/README.md), then run `node rapidroom/status.mjs --check`. Add a `highlight` only for something a new user would notice. Do not edit generated `CHANGES.md` or the README status block: release snapshots and main preview artifacts are generated by workflows, without commits to main.
- **Checks:**
  - `cargo fmt -p RapidRAW -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` in `src-tauri/`;
  - `cargo test --lib` if you touched Rust;
  - `npx prettier --check`, `npx eslint` and `npx tsc --noEmit` on the files you changed. Upstream already has lint and type errors; don't add new ones.
- **Tests: what to add (maintainer decision, 2026-10-15).** Integration, end-to-end and human acceptance checks protect RapidRoom most: the 60-render pixel regression, native UI runs of the real app, and the maintainer's review of before/after panels.
  - **Don't add trivial unit tests** that restate the code (a test for a 4-line `match` is noise to write, review and maintain).
  - **Do add reference checks** that pin behaviour to an outside source of truth: real camera files, Lensfun- or Adobe-published values, round trips through real sidecars, or a regression that has bitten us before.
  - **Reviewers:** run throwaway probes against the real functions while reviewing. Commit one only if it guards something likely to regress.
- **Rendering:**
  - The maintainers run a pixel-exact regression check: 60 renders of CC0 raws. It isn't in CI yet (issue #6).
  - If your change could alter rendered output, say so in the PR and show before/after images. Label it `rendering-change`.
  - If you can't run the check, say that too.
- **Honesty:** fill in the PR template. Say what you tested and how, and mark everything else as untested. Disclose how much AI was involved.

## Don'ts

- **Don't link upstream or fork issues and PRs** (`CyberTimon/RapidRAW#123`, `someone/fork#4`) in RapidRoom issues, PRs or commit messages. GitHub would add a "mentioned" event to their tracker. Write them as code, `` `CyberTimon/RapidRAW#123` ``. Links to commits are fine.
- **Don't @-mention people outside RapidRoom** (fork authors, upstream maintainers) in issues, PRs or commit messages. GitHub notifies them. Name them without the @, e.g. "by chuckhenrich". Maintainers decide when to contact people.
- **Don't post anything outside this repo:** no upstream PRs, comments on other repos, or forks. The maintainers decide that.
- **Don't modify or reuse `public/splash-rapidroom.jpg`.** It is all rights reserved; see its `.license` file.
- **Don't change the app identifier** (`io.github.CyberTimon.RapidRAW` in `tauri.conf.json`). It keeps existing users' settings.
- **Don't add new network connections or telemetry** without an issue labelled `network-privacy` and a maintainer's OK.
- **Don't pipe RapidRAW's CLI output through `head`**, and don't call it with unknown flags. RapidRAW has no `--help`; unknown arguments open the GUI.

## Useful context

- **Upstream style:** small, focused changes with few code comments. Upstream doesn't take tests in PRs (it asked us to remove them), so send upstream the fix only; RapidRoom keeps reference checks per the testing rule above.
- **Builds:** a full release build takes 20–50 minutes. Use `cargo check` while iterating.
