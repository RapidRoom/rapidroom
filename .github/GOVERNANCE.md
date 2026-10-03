# Governance

RapidRoom is run by its maintainers in the [RapidRoom](https://github.com/RapidRoom) GitHub organization. The project is young, so this document is short and will grow with the team.

## Maintainers

- **Now:** [@yojen7](https://github.com/yojen7), who started the project. Much of the work is done with AI agents and disclosed as such.
- **Joining:** people who contribute regularly, especially authors of RapidRAW forks, are invited as maintainers. To ask, open an issue.
- **Stepping back:** maintainers can step back at any time. Inactive maintainers (no activity for a year) move to an emeritus list in CREDITS.md and can come back on request.

## How decisions are made

- **Ordinary PRs:** one maintainer approval and green CI. The author can't approve their own PR once there is more than one maintainer.
- **Bigger decisions** need two maintainer approvals (once there are two maintainers) and stay open for at least 72 hours, so others can comment. These are:
  - changes to rendered output, and updates to the regression reference;
  - new network connections or telemetry;
  - dependency or licence changes;
  - changes to this document.
- **Disagreements:** we discuss them in the issue or PR and aim for consensus. If that fails, the maintainers vote and a simple majority decides.
- **Human sign-off:** an agent can open, review and merge ordinary PRs for a maintainer. Updating the regression reference, releases, and changes to this document need a human maintainer's explicit approval.

## Releases

Releases are tagged from `main`. Their notes list every change since the last release, with credit to each source fork and author.

## Relationship to upstream

We merge upstream RapidRAW regularly and offer our changes back. RapidRoom isn't affiliated with or endorsed by the RapidRAW project. If the two projects diverge, RapidRoom goes on independently, still under AGPL-3.0.

## Conduct

Be kind, assume good faith, and keep criticism about the code. Maintainers may hide comments or block accounts that harass people.
