// Load the built frontend (dist/) in headless Chromium under a Content-Security-Policy and
// report every violation. The Tauri bridge is a stub, so this covers what the UI loads on
// its own (scripts, styles, fetches, frames), not what the Rust side does.
//
//   npm run build
//   npm i --no-save --package-lock=false playwright-core
//   node rapidroom/validation/csp/measure.mjs [policy] [--cloud]
//
// policy: proposed (default), strict-style, current. --cloud starts with the AI provider set
// to "cloud", which mounts Clerk. CHROMIUM_PATH overrides the browser.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const { chromium } = await import(process.env.RAPIDROOM_PLAYWRIGHT_MODULE ?? 'playwright-core');

const DIST = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../dist');

const base = {
  'default-src': ["'self'"],
  'script-src': ["'self'"],
  'style-src': ["'self'", "'unsafe-inline'"],
  'img-src': ["'self'", 'asset:', 'http://asset.localhost', 'blob:', 'data:'],
  'font-src': ["'self'"],
  'connect-src': [
    "'self'",
    'ipc:',
    'http://ipc.localhost',
    'https://api.github.com',
    'https://raw.githubusercontent.com',
  ],
  'frame-src': ['https://www.openstreetmap.org'],
  'worker-src': ["'none'"],
  'object-src': ["'none'"],
  'base-uri': ["'none'"],
  'form-action': ["'none'"],
};
const serialize = (p) =>
  Object.entries(p)
    .map(([k, v]) => `${k} ${v.join(' ')}`)
    .join('; ');

const config = JSON.parse(fs.readFileSync(path.resolve(DIST, '../src-tauri/tauri.conf.json'), 'utf8'));
const configured = config.app.security.csp;
if (!configured || typeof configured !== 'object') throw new Error('Production CSP must be a directive map');
if (configured['script-src'] !== "'self'") throw new Error('Production scripts must be local only');
if (config.app.security.dangerousDisableAssetCspModification?.join(',') !== 'style-src') {
  throw new Error('Only runtime styles may opt out of Tauri CSP rewriting');
}

const POLICIES = {
  current: null,
  configured: serialize(Object.fromEntries(Object.entries(configured).map(([k, v]) => [k, v.split(' ')]))),
  proposed: serialize(base),
  'strict-style': serialize({ ...base, 'style-src': ["'self'"] }),
};

const args = process.argv.slice(2);
const cloud = args.includes('--cloud');
const probe = args.includes('--probe');
const policyName = args.find((a) => !a.startsWith('--')) ?? 'configured';
if (!(policyName in POLICIES)) throw new Error(`unknown policy ${policyName}`);
const csp = POLICIES[policyName];

const types = {
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.html': 'text/html',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
};
const server = http.createServer((req, res) => {
  let p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (p === '/') p = '/index.html';
  const file = path.resolve(DIST, `.${p}`);
  const relative = path.relative(DIST, file);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative) || !fs.existsSync(file)) {
    res.writeHead(404);
    return res.end();
  }
  const headers = { 'Content-Type': types[path.extname(file)] ?? 'application/octet-stream' };
  if (csp) headers['Content-Security-Policy'] = csp;
  res.writeHead(200, headers);
  fs.createReadStream(file).pipe(res);
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
});
const page = await browser.newPage();
const violations = new Set();
const pageErrors = [];
const clerkRequests = [];
page.on('request', (request) => {
  if (/clerk\.(accounts\.dev|com)/.test(request.url())) clerkRequests.push(request.url());
});
page.on('pageerror', (error) => pageErrors.push(error.message));
await page.exposeFunction('__reportViolation', (v) => violations.add(v));
await page.addInitScript(
  ({ cloud }) => {
    document.addEventListener('securitypolicyviolation', (e) => {
      const where = e.sourceFile ? ` (${e.sourceFile.split('/').pop()}:${e.lineNumber})` : '';
      window.__reportViolation(`${e.effectiveDirective}: ${e.blockedURI || 'inline'}${where}`);
    });
    let id = 0;
    window.__TAURI_OS_PLUGIN_INTERNALS__ = {
      platform: 'linux',
      version: '6',
      family: 'unix',
      eol: '\n',
      arch: 'x86_64',
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
      plugins: { path: { sep: '/', delimiter: ':' } },
      transformCallback: () => ++id,
      unregisterCallback() {},
      convertFileSrc: (p, protocol = 'asset') => `http://${protocol}.localhost/${encodeURIComponent(p)}`,
      async invoke(cmd) {
        if (cmd === 'load_settings') return cloud ? { aiProvider: 'cloud', rootFolders: [] } : { rootFolders: [] };
        if (cmd === 'plugin:app|version') return '2.2.0';
        if (cmd === 'plugin:os|os_type') return 'linux';
        if (cmd === 'plugin:path|resolve_directory') return '/home/user';
        if (cmd.startsWith('plugin:event|listen')) return ++id;
        if (/^(get|list|load)_/.test(cmd)) return [];
        return null;
      },
    };
  },
  { cloud },
);
await page.goto(`http://127.0.0.1:${server.address().port}/`);
await page.waitForTimeout(6000);
const rendered = await page.evaluate(() => (document.getElementById('root')?.childElementCount ?? 0) > 0);
const startupViolations = [...violations];
let remoteScriptBlocked = null;
if (probe) {
  await page.evaluate(() => {
    window.__cspProbeRan = false;
    const script = document.createElement('script');
    script.src = 'data:text/javascript,window.__cspProbeRan=true';
    document.head.appendChild(script);
  });
  await page.waitForTimeout(100);
  remoteScriptBlocked = await page.evaluate(() => window.__cspProbeRan === false);
  remoteScriptBlocked &&= [...violations].some((v) => v.startsWith('script-src-elem: data'));
}
console.log(
  JSON.stringify(
    {
      policy: policyName,
      csp,
      cloud,
      rendered,
      pageErrors,
      violations: startupViolations,
      clerkRequests,
      remoteScriptBlocked,
    },
    null,
    2,
  ),
);
await browser.close();
server.close();
if (
  !rendered ||
  pageErrors.length ||
  (policyName === 'configured' && (startupViolations.length || clerkRequests.length)) ||
  (probe && !remoteScriptBlocked)
)
  process.exitCode = 1;
