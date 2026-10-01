// Browser-preview stand-in for the Rust core — used ONLY when the page
// runs outside Tauri (window.__TAURI__ absent), to click through the UI
// in a plain browser. Canned responses: nothing is parsed, merged or
// saved, so actions succeed but the library stays as it is. Only the
// status flags change, so the lock screen and Settings toggles respond.
// It never activates inside the app.

const ago = (days) => new Date(Date.now() - days * 86400000).toISOString();
const raise = (m) => { throw m; };

// A fixture line: '# comment', '' for a blank, '!raw' for a line that
// doesn't parse, or [key, value] ([key, value, true] when exported).
const PROJECTS = [
  { id: 'p-api', name: 'lumen-api', pathHint: 'C:\\dev\\lumen\\api', lines: [
    '# Server', ['PORT', '3000'], ['LOG_LEVEL', 'debug'], '',
    '# Database', ['DATABASE_URL', 'postgres://lumen:s3cr3t@localhost:5432/lumen_dev'], ['REDIS_URL', 'redis://localhost:6379/0'], '',
    '# Auth', ['JWT_SECRET', '9f1c4f5a2e6b48d3a7c0e9b1d8f24a61'], ['STRIPE_SECRET_KEY', 'sk_test_demo-not-a-real-key', true],
    ['SENTRY_DSN', 'https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247'], '!BROKEN LINE EXAMPLE', ['LOG_LEVEL', 'trace'],
  ] },
  { id: 'p-web', name: 'lumen-web', pathHint: 'C:\\dev\\lumen\\web', lines: [
    ['VITE_API_URL', 'http://localhost:3000'], ['LOG_LEVEL', 'warn'],
    ['SENTRY_DSN', 'https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247'],
  ] },
  { id: 'p-tools', name: 'tooling-scripts', pathHint: null, lines: [
    '# Personal automation', ['GITHUB_TOKEN', 'ghp_demo-not-a-real-token'],
    ['DATABASE_URL', 'postgres://tools:tools@localhost:5432/scratch'],
  ] },
];
const SNAPSHOTS = [
  { id: 's-latest', capturedAt: ago(1), via: 'file' },
  { id: 's-older', capturedAt: ago(6), via: 'paste' },
];

const proj = (id) => PROJECTS.find((p) => p.id === id) || raise('project not found');
const entries = (p) => p.lines.filter(Array.isArray);
const valueOf = (p, key) => entries(p).filter(([k]) => k === key).pop()?.[1];
const keys = (p) => [...new Set(entries(p).map(([k]) => k))];
const meta = (p) => ({
  id: p.id, name: p.name, pathHint: p.pathHint, snapshotCount: SNAPSHOTS.length,
  entryCount: keys(p).length, latestCapturedAt: SNAPSHOTS[0].capturedAt,
  sharedKeys: keys(p).filter((k) => PROJECTS.some((o) => o !== p && valueOf(o, k) !== undefined)).length,
});
const lineView = (p) => (l, idx) => {
  if (!Array.isArray(l)) return l === '' ? { t: 'blank' } : l[0] === '!' ? { t: 'bad', idx } : { t: 'comment', text: l };
  const [key, value, exported = false] = l;
  const overridden = p.lines.findLastIndex((x) => Array.isArray(x) && x[0] === key) !== idx;
  const reuse = PROJECTS.filter((o) => o !== p && valueOf(o, key) !== undefined)
    .map((o) => ({ projectId: o.id, name: o.name, same: valueOf(o, key) === value }));
  return { t: 'entry', idx, key, exported, overridden, reuse };
};
const editLine = (l) =>
  !Array.isArray(l) ? (l === '' ? { kind: 'blank' } : l[0] === '!' ? { kind: 'bad', raw: l.slice(1) } : { kind: 'comment', text: l })
  : { kind: 'entry', key: l[0], value: l[1], exported: !!l[2] };

const S = { encrypted: false, locked: false, autoUpdateCheck: false, updateAvailable: null };
const DIR = 'C:\\dev\\lumen\\api';
const target = { token: 'mock-target', path: `${DIR}\\.env.local`, dir: DIR, class: 'writable', exists: true };
const PREVIEW = { resultEntryCount: 11, added: ['JWT_SECRET'], substituted: ['PORT', 'LOG_LEVEL'], emptied: [], kept: ['LOCAL_ONLY'], blocked: null };
const EXAMPLE_PREVIEW = { resultEntryCount: 10, added: ['REDIS_URL'], substituted: ['PORT', 'DATABASE_URL'], emptied: ['MAILER_URL'], kept: [], blocked: null };
const CAPTURED = { projectId: 'p-api', snapshotId: 's-latest', entryCount: 3 };
const IMPORT_PATH = 'C:\\Users\\you\\Downloads\\envarsa-old-laptop.store';

const COMMANDS = {
  store_status: () => ({
    state: S.locked ? 'locked' : 'unlocked',
    storePath: 'C:\\Users\\you\\AppData\\Roaming\\com.envarsa.app\\envarsa.store',
    encrypted: S.encrypted, envOverride: false, customLocation: false, backupExists: true,
    projectCount: PROJECTS.length, error: null, appVersion: '0.1.0-mock',
    updateAvailable: S.updateAvailable, autoUpdateCheck: S.autoUpdateCheck, channel: 'direct',
  }),
  unlock: ({ passphrase }) => { if (passphrase !== 'demo') raise('wrong passphrase'); S.locked = false; },
  lock: () => { S.locked = true; },
  list_projects: () => PROJECTS.map(meta),
  get_project: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = SNAPSHOTS.find((x) => x.id === snapshotId) || SNAPSHOTS[0];
    return {
      id: p.id, name: p.name, pathHint: p.pathHint,
      snapshots: SNAPSHOTS.map((x) => ({ ...x, entryCount: keys(p).length })),
      snapshotId: s.id, capturedAt: s.capturedAt, via: s.via, isLatest: s === SNAPSHOTS[0],
      entryCount: keys(p).length, lines: p.lines.map(lineView(p)),
    };
  },
  preview_capture: ({ text }) => ({ entries: text.trim() ? 3 : 0, comments: 1, bad: 0, dupKeys: [] }),
  pick_env_file: () => ({
    token: 'mock-source', path: 'C:\\dev\\sample\\.env', dir: 'C:\\dev\\sample', nameGuess: 'sample',
    text: '# Sample\nAPI_KEY=abc123\nDEBUG=true\n',
  }),
  capture: () => CAPTURED,
  reveal_value: ({ projectId, idx }) => {
    const l = proj(projectId).lines[idx];
    if (Array.isArray(l)) return { key: l[0], value: l[1] };
    if (l?.[0] === '!') return { key: '', value: l.slice(1) };
    raise('that line has no value to reveal');
  },
  copy_value: ({ projectId, idx }) => proj(projectId).lines[idx][0],
  copy_block: ({ projectId }) => keys(proj(projectId)).length,
  export_snapshot: () => 'C:\\dev\\exported.env',
  stage_write_target: () => target,
  pick_write_target: () => target,
  pick_example_file: () => ({
    token: 'mock-example', exampleName: '.env.example', outPath: `${DIR}\\.env.local`,
    outClass: 'writable', exampleKeys: ['PORT', 'DATABASE_URL', 'MAILER_URL'],
  }),
  preview_write: ({ token }) => (token === 'mock-example' ? EXAMPLE_PREVIEW : PREVIEW),
  write_env_local: () => `${DIR}\\.env.local`,
  edit_lines: ({ projectId }) => proj(projectId).lines.map(editLine),
  save_edited_snapshot: () => CAPTURED,
  update_project: () => {},
  delete_project: () => {},
  restore_snapshot: () => 's-latest',
  enable_encryption: ({ passphrase }) => {
    if (passphrase.length < 8) raise('use at least 8 characters');
    S.encrypted = true;
  },
  change_passphrase: () => {},
  disable_encryption: () => { S.encrypted = false; },
  reveal_store: () => {},
  move_store_to_default: () => 'D:\\sync\\envarsa.store',
  export_store: () => 'C:\\Users\\you\\Desktop\\envarsa.store',
  pick_import_store: () => 'mock-import',
  // The fixture import is encrypted with the passphrase "demo".
  inspect_import: ({ token, passphrase }) => {
    if (!passphrase) return { token, path: IMPORT_PATH, encrypted: true, unlocked: false, projects: [] };
    if (passphrase !== 'demo') raise('wrong passphrase');
    return {
      token, path: IMPORT_PATH, encrypted: true, unlocked: true,
      projects: [
        { name: 'lumen-api', snapshotCount: 1, entryCount: 3, latestCapturedAt: ago(9), conflictsWith: 'lumen-api' },
        { name: 'billing-svc', snapshotCount: 1, entryCount: 2, latestCapturedAt: ago(9), conflictsWith: null },
      ],
    };
  },
  apply_import: () => ({ added: 1, replaced: 0, renamed: 1, skipped: 0 }),
  restore_backup: () => raise('the store loaded fine — restoring the backup is only for when it cannot be read'),
  // Always "finds" an update, so the available state is clickable.
  check_for_updates: () => { S.updateAvailable = '9.9.9'; return { latest: '9.9.9', updateAvailable: true }; },
  set_auto_update_check: ({ enabled }) => { S.autoUpdateCheck = enabled; },
  open_releases_page: () => {},
  ui_log: ({ level, message }) => console.log(`[ui:${level}]`, message),
};

export function installMock() {
  console.warn('Envarsa UI running with the browser-preview mock (no Tauri).');
  window.__TAURI__ = {
    core: {
      invoke: (cmd, args = {}) =>
        new Promise((resolve) => resolve((COMMANDS[cmd] || raise(`mock: unknown command ${cmd}`))(args))),
    },
    event: { listen: () => Promise.resolve(() => {}) },
  };
}
