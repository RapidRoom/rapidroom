// RapidRoom's version and links. RAPIDROOM_VERSION must equal `version` in src-tauri/tauri.conf.json
// (a test checks this); that is the app and package version everywhere. Releases are tagged
// v<RAPIDROOM_VERSION>; see rapidroom/RELEASING.md for when to bump which number.
export const RAPIDROOM_VERSION = '2.2.0';
// The upstream RapidRAW release RapidRoom is based on. Update it when an upstream sync brings in a
// new upstream release (upstream's own version bump in tauri.conf.json is not taken).
export const RAPIDRAW_BASE_VERSION = '1.6.4';
export const RAPIDROOM_REPO_URL = 'https://github.com/RapidRoom/rapidroom';
export const UPSTREAM_REPO_URL = 'https://github.com/CyberTimon/RapidRAW';
