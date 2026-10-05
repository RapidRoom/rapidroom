# CSP measurement

`measure.mjs` serves the built frontend (`dist/`) under a Content-Security-Policy, loads it in headless Chromium with a stub Tauri bridge, and lists every violation and page error. The `configured` mode reads the shipped policy from `src-tauri/tauri.conf.json`, requires local-only scripts and the styles-only Tauri rewrite exception, and fails on a blank start screen, page errors, startup policy violations, or Clerk requests. `--probe` also requires a nonlocal `data:` script to be blocked. The historical control modes retain their expected violations. It backs the CSP section of [`../../TERMINAL.md`](../../TERMINAL.md) (issue #119).

```sh
npm run build
npm i --no-save --package-lock=false playwright-core
node rapidroom/validation/csp/measure.mjs configured --probe # shipped policy plus blocked script probe
node rapidroom/validation/csp/measure.mjs strict-style      # same, without 'unsafe-inline' for styles
node rapidroom/validation/csp/measure.mjs current           # no CSP, as shipped
node rapidroom/validation/csp/measure.mjs configured --cloud --probe # saved cloud setting, with Clerk disabled
```

`RAPIDROOM_PLAYWRIGHT_MODULE` optionally names an existing Playwright module, so local checks need not install into shared dependencies. The Production CSP workflow installs pinned Playwright 1.63.0 into its own temporary directory and runs both configured cases. `CHROMIUM_PATH` overrides the browser (default: the cloud sessions' `/opt/pw-browsers` Chromium).

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

## Terminal prerequisite recheck (2026-10-04)

With Clerk disabled, both the normal start screen and a saved `aiProvider: cloud` setting render under the configured production policy with zero page errors, zero startup policy violations and zero Clerk requests. Both blocked-script probes pass in local Chromium. TypeScript diagnostics match base `c8b58802` after normalizing line/column positions and the i18next type-union elision count for two added keys; changed-file ESLint adds no diagnostics. Native Tauri CSP rewriting and the full editor remain pending validation before the PTY is enabled.
