# Contributing to RapidRoom

Thanks for helping. RapidRoom collects the best work from across the RapidRAW community, so contributions come in two kinds: your own changes, and **harvests** of work from other forks.

## AI-assisted work is welcome

You can use any tools you like, including AI agents that write the whole change. We ask for three things:

1. **Say how it was made.** Tick the right box in the pull request template. Honest disclosure is never held against a PR.
2. **Someone answers for it.** A human or a clearly identified agent operator has to respond to review and fix problems. Unattended drive-by PRs will be closed.
3. **It passes the same gates as everything else.** How a change was written doesn't change how it's judged.

## The gates

Every PR, ours or harvested, needs:

- **CI green:** the build, `npm run format:check`, `npm run lint`, `npm run i18n:check`, `cargo fmt --check` and `cargo clippy -D warnings` (the existing upstream workflows).
- **Rendering unchanged, or changed on purpose.** A pixel-exact regression check on a small CC0 raw corpus is being added to CI. Until then, a maintainer runs it locally. A PR that changes rendered output on purpose says so and shows before/after images. Updating the reference renders needs sign-off from a human maintainer.
- **A description a reviewer can check:** what changed, why, and how it was tested (OS, GPU, cameras). Mark anything untested as untested.
- **One change per PR.** Small PRs get merged faster.

## Harvesting from other forks

RapidRAW and all its forks are AGPL-3.0, so good work can be brought in even if its author doesn't join. When you do:

- **Keep authorship.** Cherry-pick or merge with the original author on the commit. If you squash or adapt it, add a `Co-authored-by:` trailer and a "from <fork>@<commit>" line.
- **Keep notices.** Leave copyright and licence headers as they are.
- **Credit it.** Add the fork and author to [CREDITS.md](../CREDITS.md) in the same PR.
- **Tell the author.** A short, friendly note on their repo, and offer any improvements back.

## Upstream first, where it fits

If a fix belongs in RapidRAW itself, please offer it upstream too, or say in the PR that we may. RapidRoom tracks upstream `main`, so anything merged there comes back to us for free.

## Reporting bugs

Open an issue here with the RapidRoom version or commit, your OS and GPU, and the camera model. Attach a raw file if you can share it. If the bug is also in stock RapidRAW, say so.

## Licence

By contributing, you agree that your contribution is licensed under AGPL-3.0.
