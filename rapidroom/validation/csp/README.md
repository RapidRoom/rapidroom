# CSP measurement

`measure.mjs` serves the built frontend (`dist/`) under a Content-Security-Policy, loads it in headless Chromium with a stub Tauri bridge, and lists every violation and page error. It returns a nonzero exit code if the start screen does not render. It backs the CSP section of [`../../TERMINAL.md`](../../TERMINAL.md) (issue #119).

```sh
npm run build
npm i --no-save --package-lock=false playwright-core
node rapidroom/validation/csp/measure.mjs proposed          # the policy proposed in TERMINAL.md
node rapidroom/validation/csp/measure.mjs strict-style      # same, without 'unsafe-inline' for styles
node rapidroom/validation/csp/measure.mjs current           # no CSP, as shipped
node rapidroom/validation/csp/measure.mjs proposed --cloud  # AI provider set to "cloud" (mounts Clerk)
```

`CHROMIUM_PATH` overrides the browser (default: the cloud sessions' `/opt/pw-browsers` Chromium).

## Results (2026-10-04, `main` at 24b2cb3)

| Policy         | Cloud AI | Start screen renders | Violations                                                                                                  |
| -------------- | -------- | -------------------- | ----------------------------------------------------------------------------------------------------------- |
| `current`      | off      | yes                  | none (no policy)                                                                                            |
| `proposed`     | off      | yes                  | **none**                                                                                                    |
| `strict-style` | off      | yes                  | `style-src-elem: inline` (react-toastify's runtime `<style>`)                                               |
| `proposed`     | on       | yes                  | `script-src-elem: https://brief-seasnail-12.clerk.accounts.dev/npm/@clerk/clerk-js@6/dist/clerk.browser.js` |

## Local integration recheck (2026-10-04)

The production frontend at `d364a964` (current integration, including #128/#106/#104/#98/#123) reproduces all four rows above in headless Chromium. Every start screen rendered, with no page errors. `current` and `proposed` had no violations; `strict-style` blocked the inline runtime stylesheet; `proposed --cloud` blocked Clerk's remote script. This uses the same stub bridge and has the limits below. It is not a native desktop or terminal-launch test.

## Limits

- The bridge is a stub: no folder is open, so the editor, masks, GPS map and community page aren't exercised. TERMINAL.md covers those from a static review of `src/` and the built bundle.
- The page is served from `http://127.0.0.1`, not `tauri://localhost`, so `'self'` means the test server. Tauri's own CSP rewriting (script hashes, nonces) isn't applied.
- Rust-side network requests aren't governed by the CSP and aren't covered.
