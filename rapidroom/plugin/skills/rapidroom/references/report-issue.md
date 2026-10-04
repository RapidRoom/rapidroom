# Improving RapidRoom: drafting an issue

When the user hits a defect (wrong output, a crash, an MCP error that shouldn't happen) or a limitation (a missing MCP tool, docs that didn't answer their question), offer to report it to RapidRoom. **Nothing is posted without the user's explicit yes.**

## 1. Check it's real

- Reproduce it once if you can, and note the exact tool call and error.
- Check the version (see [source.md](source.md)) and, for a limitation, whether it's already listed as [coming](tools.md#coming). A planned tool isn't a new issue; tell the user which issue tracks it.

## 2. Search existing issues first

```sh
gh issue list -R RapidRoom/rapidroom --state all --search "<key words>" --limit 20
```

Without `gh`, search on the web: `https://github.com/RapidRoom/rapidroom/issues?q=<key+words>`. If one matches, show it to the user and suggest adding to it (a comment or a 👍) instead of a new issue. Adding a comment needs the same yes as a new issue.

## 3. Draft it

```markdown
**What happened**
<one or two sentences>

**Steps to reproduce**

1. …

**Expected**
…

**Actual**
… (exact error text or tool result, shortened)

**What's missing** (for limitations)
<the tool, option or docs that would have helped, and what the user was trying to do>

**Environment**
RapidRoom <version> (<release / source checkout at commit>), <OS and version>, <GPU if relevant>, camera <model> if relevant, assistant <Claude Code / Codex>.

---

_Drafted with an AI assistant and reviewed by the reporter before posting._
```

- Title: short and specific, for example `MCP: update_adjustments rejects hsl.aquas`.
- **Privacy**: no photos, file names or paths from the user's machine unless they agree. Replace them (`~/Pictures/2026/IMG_1234.CR3` → `a Canon CR3 file`). No EXIF serial numbers, GPS or personal names.
- Don't @-mention anyone, and don't link issues of other repositories: write them as code (`` `CyberTimon/RapidRAW#123` ``).
- Keep it to one problem. Two problems, two drafts.

## 4. Show the draft and wait

Show the full title and body, say where it will go (`RapidRoom/rapidroom`, label `from-assistant`), and ask: "Shall I post this?" Only an explicit yes counts: "yes", "post it", "go ahead". Silence, "looks good" about a different question, or an earlier yes for another draft don't. If the user edits the draft, show the new version and ask again.

## 5. Post

Check whether `gh` is logged in: `gh auth status`.

- **Logged in**: write the body to a temporary file and run

  ```sh
  gh issue create -R RapidRoom/rapidroom --title "<title>" --body-file <file> --label from-assistant
  ```

  If GitHub refuses the label (only people with triage rights may set labels), post again without `--label`; the footer line tells the maintainers. Give the user the issue link.

- **Not logged in** (the default): open a pre-filled new-issue page in the browser. The `from-assistant` template adds the label for everyone:

  ```
  https://github.com/RapidRoom/rapidroom/issues/new?template=from-assistant.md&title=<url-encoded title>&body=<url-encoded body>
  ```

  Open it with `xdg-open` (Linux) or `open` (macOS), or give the user the link. The user posts it themselves from the page. If the URL would be longer than about 6,000 characters, shorten the body or put it on the clipboard and open the page without `body`.

Never post to other repositories, upstream RapidRAW or forks. Never create the issue without the yes, not even as a draft or a "test".
