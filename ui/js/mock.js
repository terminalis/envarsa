// Browser-preview mock of the IPC surface — used ONLY when the page
// runs outside Tauri (window.__TAURI__ absent), e.g. while iterating on
// the UI in a plain browser. The Rust core is the authoritative
// implementation; this mirrors just enough behavior to click through
// every surface with fixture data. It never activates inside the app.

const now = () => new Date().toISOString();
let idSeq = 1;
const nid = () => `mock-${idSeq++}`;

// --- minimal mirror of envfile.rs ----------------------------------
function parseValue(v) {
  if (v.length >= 2 && v.startsWith('"') && v.endsWith('"')) {
    return v
      .slice(1, -1)
      .replace(/\\([nrt"\\])/g, (_, c) => ({ n: '\n', r: '\r', t: '\t', '"': '"', '\\': '\\' }[c]));
  }
  if (v.length >= 2 && v.startsWith("'") && v.endsWith("'")) return v.slice(1, -1);
  const cut = v.indexOf(' #');
  return cut >= 0 ? v.slice(0, cut).trimEnd() : v;
}

function parseEnv(raw) {
  return raw.replace(/^﻿/, '').split(/\r?\n/).map((line, i, arr) => {
    // str::lines() in Rust drops the empty segment after a trailing \n.
    if (i === arr.length - 1 && line === '' && raw.endsWith('\n')) return null;
    const t = line.trim();
    if (!t) return { t: 'blank' };
    if (t.startsWith('#')) return { t: 'comment', text: t };
    let body = t, exported = false;
    if (t.startsWith('export ')) { body = t.slice(7).trimStart(); exported = true; }
    const eq = body.indexOf('=');
    if (eq < 0) return { t: 'bad', raw: line };
    const key = body.slice(0, eq).trim();
    if (!key || /[\s#"']/.test(key)) return { t: 'bad', raw: line };
    return { t: 'entry', key, value: parseValue(body.slice(eq + 1).trim()), exported };
  }).filter(Boolean);
}

const effective = (lines) => {
  const map = new Map();
  for (const l of lines) if (l.t === 'entry') map.set(l.key, l.value);
  return map;
};

// minimal mirror of envfile::format_value / serialize_lines
function formatValue(v) {
  if (v === '') return '';
  const matchedQuotes =
    v.length >= 2 && ((v[0] === '"' && v.endsWith('"')) || (v[0] === "'" && v.endsWith("'")));
  const safe = v.trim() === v && !v.includes(' #') && !/[\n\r\t]/.test(v) && !matchedQuotes;
  if (safe) return v;
  if (!v.includes("'") && !/[\n\r\t]/.test(v)) return `'${v}'`;
  return (
    '"' +
    v.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r').replace(/\t/g, '\\t') +
    '"'
  );
}
const serializeLine = (l) =>
  l.kind === 'blank' ? ''
  : l.kind === 'comment' ? l.text
  : l.kind === 'bad' ? l.raw
  : `${l.exported ? 'export ' : ''}${l.key}=${formatValue(l.value)}`;
const serializeLines = (lines) => lines.map((l) => serializeLine(l) + '\n').join('');

// At most one .env.local write is staged at a time (mirrors pending_write).
let writeStage = null;

// --- fixture store ---------------------------------------------------
const FIX = (name, hint, raws) => ({
  id: nid(),
  name,
  pathHint: hint,
  createdAt: now(),
  snapshots: raws.map((raw, i) => ({
    id: nid(),
    capturedAt: new Date(Date.now() - (raws.length - 1 - i) * 86400000 * 3 - 7200000).toISOString(),
    via: i === 0 ? 'file' : 'paste',
    sourcePath: i === 0 ? `${hint}\\.env` : null,
    raw,
  })),
});

const DB = {
  encrypted: false,
  locked: false,
  autoUpdateCheck: false,
  updateAvailable: null,
  projects: [
    FIX('lumen-api', 'C:\\dev\\lumen\\api', [
      '# Server\nPORT=3000\nLOG_LEVEL=info\n\n# Database\nDATABASE_URL=postgres://lumen:s3cr3t@localhost:5432/lumen_dev\nREDIS_URL=redis://localhost:6379/0\n',
      '# Server\nPORT=3000\nLOG_LEVEL=debug\n\n# Database\nDATABASE_URL=postgres://lumen:s3cr3t@localhost:5432/lumen_dev\nREDIS_URL=redis://localhost:6379/0\n\n# Auth\nJWT_SECRET=9f1c4f5a2e6b48d3a7c0e9b1d8f24a61\nSESSION_TTL_HOURS=72\n\n# Stripe (test mode)\nSTRIPE_SECRET_KEY=sk_test_demo-not-a-real-key\nSTRIPE_WEBHOOK_SECRET=whsec_8a2f0d9c1b3e4f5a6d7c8b9a0e1f2d3c\n\n# Observability\nSENTRY_DSN=https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247\nBROKEN LINE EXAMPLE\nLOG_LEVEL=trace\n',
    ]),
    FIX('lumen-web', 'C:\\dev\\lumen\\web', [
      'VITE_API_URL=http://localhost:3000\nVITE_STRIPE_PUBLISHABLE_KEY=pk_test_TYooMQauvdEDq54NiTphI7jx\nLOG_LEVEL=warn\nSENTRY_DSN=https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247\n',
    ]),
    FIX('tooling-scripts', 'C:\\dev\\tooling', [
      '# Personal automation\nGITHUB_TOKEN=ghp_demo-not-a-real-token\nOPENAI_API_KEY=sk-proj-demo-not-a-real-key-000000000000\nDATABASE_URL=postgres://tools:tools@localhost:5432/scratch\n',
    ]),
  ],
};

// What the fixture "import file" holds (inspect/apply_import).
const IMPORT_RAWS = {
  'lumen-api': [
    '# from the old laptop\nPORT=4000\nLOG_LEVEL=info\nDATABASE_URL=postgres://lumen:s3cr3t@db.internal:5432/lumen_prod\n',
  ],
  'billing-svc': [
    'BILLING_URL=http://localhost:7000\nSTRIPE_SECRET_KEY=sk_test_demo-not-a-real-key\n',
  ],
};

const proj = (id) => DB.projects.find((p) => p.id === id) || raise('project not found');
const snap = (p, id) => p.snapshots.find((s) => s.id === id) || raise('snapshot not found');
const latest = (p) => p.snapshots[p.snapshots.length - 1];
const raise = (m) => { throw m; };

const COMMANDS = {
  store_status: () => ({
    state: DB.locked ? 'locked' : 'unlocked',
    storePath: 'C:\\Users\\you\\AppData\\Roaming\\com.envarsa.app\\envarsa.store',
    encrypted: DB.encrypted,
    envOverride: false,
    backupExists: true,
    projectCount: DB.projects.length,
    error: null,
    appVersion: '0.1.0-mock',
    updateAvailable: DB.updateAvailable,
    autoUpdateCheck: DB.autoUpdateCheck,
  }),
  unlock: ({ passphrase }) => {
    if (passphrase !== 'demo') raise('wrong passphrase');
    DB.locked = false;
  },
  lock: () => { DB.locked = true; },
  list_projects: () =>
    DB.projects
      .map((p) => {
        const mine = effective(parseEnv(latest(p).raw));
        let shared = 0;
        for (const k of mine.keys()) {
          if (DB.projects.some((o) => o.id !== p.id && effective(parseEnv(latest(o).raw)).has(k))) shared++;
        }
        return {
          id: p.id, name: p.name, pathHint: p.pathHint,
          snapshotCount: p.snapshots.length,
          entryCount: mine.size,
          latestCapturedAt: latest(p).capturedAt,
          sharedKeys: shared,
        };
      })
      .sort((a, b) => a.name.localeCompare(b.name)),
  get_project: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snapshotId ? snap(p, snapshotId) : latest(p);
    const lines = parseEnv(s.raw);
    const lastIdx = new Map();
    lines.forEach((l, i) => { if (l.t === 'entry') lastIdx.set(l.key, i); });
    const others = DB.projects.filter((o) => o.id !== p.id).map((o) => [o, effective(parseEnv(latest(o).raw))]);
    return {
      id: p.id, name: p.name, pathHint: p.pathHint, createdAt: p.createdAt,
      snapshots: [...p.snapshots].reverse().map((x) => ({
        id: x.id, capturedAt: x.capturedAt, via: x.via, sourcePath: x.sourcePath,
        entryCount: effective(parseEnv(x.raw)).size,
      })),
      snapshotId: s.id, capturedAt: s.capturedAt, via: s.via, sourcePath: s.sourcePath,
      isLatest: s.id === latest(p).id,
      entryCount: effective(parseEnv(s.raw)).size,
      lines: lines.map((l, idx) => {
        // Bad lines cross masked: idx only, no raw text (see reveal_value).
        if (l.t !== 'entry') return l.t === 'bad' ? { t: 'bad', idx } : l;
        return {
          t: 'entry', idx, key: l.key, exported: l.exported,
          overridden: lastIdx.get(l.key) !== idx,
          reuse: others.filter(([, e]) => e.has(l.key)).map(([o, e]) => ({
            projectId: o.id, name: o.name, same: e.get(l.key) === l.value,
          })),
        };
      }),
    };
  },
  preview_capture: ({ text }) => {
    const lines = parseEnv(text);
    const keys = [], dup = [];
    let comments = 0, bad = 0;
    for (const l of lines) {
      if (l.t === 'comment') comments++;
      else if (l.t === 'bad') bad++;
      else if (l.t === 'entry') {
        if (keys.includes(l.key)) { if (!dup.includes(l.key)) dup.push(l.key); }
        else keys.push(l.key);
      }
    }
    return { entries: keys.length, comments, bad, keys, dupKeys: dup };
  },
  pick_env_file: () => ({
    path: 'C:\\dev\\sample\\.env',
    dir: 'C:\\dev\\sample',
    nameGuess: 'sample',
    text: 'API_KEY=abc123\nDEBUG=true\n',
  }),
  capture: ({ args }) => {
    const snapshot = {
      id: nid(), capturedAt: now(),
      via: args.sourcePath ? 'file' : 'paste',
      sourcePath: args.sourcePath, raw: args.text,
    };
    let p = args.projectId ? proj(args.projectId)
      : DB.projects.find((x) => x.name.toLowerCase() === (args.projectName || '').trim().toLowerCase());
    if (!p) {
      p = { id: nid(), name: args.projectName.trim(), pathHint: args.pathHint, createdAt: now(), snapshots: [] };
      DB.projects.push(p);
    }
    if (args.pathHint) p.pathHint = args.pathHint;
    p.snapshots.push(snapshot);
    return { projectId: p.id, snapshotId: snapshot.id, entryCount: effective(parseEnv(args.text)).size };
  },
  reveal_value: ({ projectId, snapshotId, idx }) => {
    const l = parseEnv(snap(proj(projectId), snapshotId).raw)[idx];
    if (l && l.t === 'entry') return { key: l.key, value: l.value };
    if (l && l.t === 'bad') return { key: '', value: l.raw };
    raise('that line has no value to reveal');
  },
  copy_value: ({ projectId, snapshotId, idx }) => {
    const l = parseEnv(snap(proj(projectId), snapshotId).raw)[idx];
    return l.key;
  },
  copy_block: ({ projectId, snapshotId }) => effective(parseEnv(snap(proj(projectId), snapshotId).raw)).size,
  export_snapshot: () => 'C:\\dev\\exported.env',
  export_to_path: () => {},

  // --- write .env.local (no real filesystem; targets read as "fresh") ---
  stage_write_target: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snapshotId ? snap(p, snapshotId) : latest(p);
    const dir = (s.sourcePath ? s.sourcePath.replace(/[\\/][^\\/]+$/, '') : p.pathHint) || 'C:\\dev\\project';
    const path = `${dir}\\.env.local`;
    writeStage = { token: 'mock-write-token', path, template: null };
    return { token: writeStage.token, path, dir, class: 'writable', exists: false };
  },
  pick_write_target: ({ suggestedDir }) => {
    const dir = suggestedDir || 'C:\\dev\\project';
    const path = `${dir}\\.env.local`;
    writeStage = { token: 'mock-write-token', path, template: null };
    return { token: writeStage.token, path, dir, class: 'writable', exists: false };
  },
  preview_write: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snapshotId ? snap(p, snapshotId) : latest(p);
    const source = [...effective(parseEnv(s.raw)).keys()];
    return { resultEntryCount: source.length, added: source, substituted: [], emptied: [], kept: [], blocked: null, mode: 'fresh' };
  },
  write_env_local: () => (writeStage?.path || 'C:\\dev\\project\\.env.local'),
  pick_example_file: () => {
    const dir = 'C:\\dev\\project';
    writeStage = { token: 'mock-example-token', path: `${dir}\\.env.local`, template: '# API\nAPI_KEY=your-key-here\nPORT=3000\n' };
    return {
      token: writeStage.token,
      exampleName: '.env.example',
      outPath: writeStage.path,
      outClass: 'writable',
      outExists: false,
      exampleKeys: ['API_KEY', 'PORT'],
      exampleComments: 1,
    };
  },
  preview_example_write: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snapshotId ? snap(p, snapshotId) : latest(p);
    const source = effective(parseEnv(s.raw));
    const tplKeys = [...effective(parseEnv(writeStage?.template || '')).keys()];
    const substituted = tplKeys.filter((k) => source.has(k));
    const emptied = tplKeys.filter((k) => !source.has(k));
    const added = [...source.keys()].filter((k) => !tplKeys.includes(k));
    return { resultEntryCount: tplKeys.length + added.length, added, substituted, emptied, kept: [], blocked: null, mode: 'example' };
  },
  write_example_scaffold: () => (writeStage?.path || 'C:\\dev\\project\\.env.local'),

  // --- structured editor ---
  edit_lines: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snapshotId ? snap(p, snapshotId) : latest(p);
    return parseEnv(s.raw).map((l) =>
      l.t === 'entry' ? { kind: 'entry', key: l.key, value: l.value, exported: l.exported }
      : l.t === 'comment' ? { kind: 'comment', text: l.text }
      : l.t === 'bad' ? { kind: 'bad', raw: l.raw }
      : { kind: 'blank' });
  },
  save_edited_snapshot: ({ args }) => {
    for (const l of args.lines) {
      if (l.kind === 'entry') {
        const k = (l.key || '').trim();
        if (!k || /[\s#"']/.test(k)) raise(`"${l.key}" is not a valid key — keys can't be empty or contain spaces, #, ", or '`);
      }
    }
    const norm = args.lines.map((l) =>
      l.kind === 'comment'
        ? { ...l, text: l.text.trim().startsWith('#') ? l.text.trim() : `# ${l.text.trim()}` }
        : l);
    const raw = serializeLines(norm);
    let p = args.projectId ? proj(args.projectId)
      : DB.projects.find((x) => x.name.toLowerCase() === (args.projectName || '').trim().toLowerCase());
    if (!p) {
      p = { id: nid(), name: (args.projectName || '').trim(), pathHint: args.pathHint || null, createdAt: now(), snapshots: [] };
      DB.projects.push(p);
    }
    if (args.pathHint) p.pathHint = args.pathHint;
    const snapshot = { id: nid(), capturedAt: now(), via: 'edit', sourcePath: null, raw };
    p.snapshots.push(snapshot);
    return { projectId: p.id, snapshotId: snapshot.id, entryCount: effective(parseEnv(raw)).size };
  },
  rename_project: ({ projectId, name }) => {
    if (DB.projects.some((p) => p.id !== projectId && p.name.toLowerCase() === name.trim().toLowerCase()))
      raise(`a project named "${name}" already exists`);
    proj(projectId).name = name.trim();
  },
  set_path_hint: ({ projectId, pathHint }) => { proj(projectId).pathHint = pathHint.trim() || null; },
  delete_project: ({ projectId }) => { DB.projects = DB.projects.filter((p) => p.id !== projectId); },
  promote_snapshot: ({ projectId, snapshotId }) => {
    const p = proj(projectId);
    const s = snap(p, snapshotId);
    const c = { id: nid(), capturedAt: now(), via: 'restore', sourcePath: s.sourcePath, raw: s.raw };
    p.snapshots.push(c);
    return c.id;
  },
  enable_encryption: ({ passphrase }) => {
    if (passphrase.length < 8) raise('use at least 8 characters');
    DB.encrypted = true;
  },
  change_passphrase: () => {},
  disable_encryption: () => { DB.encrypted = false; },
  reveal_store: () => {},
  relocate_store: () => 'D:\\sync\\envarsa.store',
  export_store: ({ passphrase }) => {
    if (passphrase != null && passphrase.length < 8) raise('use at least 8 characters');
    return `C:\\Users\\you\\Desktop\\envarsa-${new Date().toISOString().slice(0, 10)}.store`;
  },
  // The picker hands back a token; the path is display-only metadata.
  pick_import_store: () => 'mock-import-token',
  // The fixture import is "encrypted" with passphrase: demo
  inspect_import: ({ token, passphrase }) => {
    if (token !== 'mock-import-token') raise('that import is no longer pending — pick the store file again');
    const path = 'C:\\Users\\you\\Downloads\\envarsa-old-laptop.store';
    if (!passphrase) return { token, path, encrypted: true, unlocked: false, projects: [] };
    if (passphrase !== 'demo') raise('wrong passphrase');
    return {
      token,
      path,
      encrypted: true,
      unlocked: true,
      projects: Object.entries(IMPORT_RAWS).map(([name, raws]) => ({
        name,
        snapshotCount: raws.length,
        entryCount: effective(parseEnv(raws[raws.length - 1])).size,
        latestCapturedAt: new Date(Date.now() - 86400000 * 9).toISOString(),
        conflictsWith:
          DB.projects.find((p) => p.name.trim().toLowerCase() === name.toLowerCase())?.name ?? null,
      })),
    };
  },
  apply_import: ({ decisions }) => {
    const sum = { added: 0, replaced: 0, renamed: 0, skipped: 0 };
    for (const [name, raws] of Object.entries(IMPORT_RAWS)) {
      const d = decisions.find((x) => x.name.toLowerCase() === name.toLowerCase());
      if (!d) raise(`no decision for incoming project "${name}"`);
      if (d.action === 'skip') {
        sum.skipped++;
        continue;
      }
      const incoming = FIX(name, 'C:\\dev\\old-laptop\\' + name, raws);
      if (d.action === 'replace') {
        DB.projects = DB.projects.filter((p) => p.name.toLowerCase() !== name.toLowerCase());
        sum.replaced++;
      } else if (d.action === 'rename') {
        incoming.name = (d.newName || '').trim();
        sum.renamed++;
      } else {
        sum.added++;
      }
      DB.projects.push(incoming);
    }
    return sum;
  },
  restore_backup: () => raise('the store loaded fine — restoring the backup is only for when it cannot be read'),
  // The mock always "finds" an update so the available-state UI is
  // clickable in the browser preview.
  check_for_updates: () => {
    DB.updateAvailable = '9.9.9';
    return { current: '0.1.0-mock', latest: '9.9.9', updateAvailable: true };
  },
  set_auto_update_check: ({ enabled }) => { DB.autoUpdateCheck = enabled; },
  open_releases_page: () => {},
  ui_log: ({ level, message }) => console.log(`[ui:${level}]`, message),
  selftest_enabled: () => false,
};

export function installMock() {
  console.warn('Envarsa UI running with the browser-preview mock (no Tauri).');
  window.__TAURI__ = {
    core: {
      invoke: (cmd, args = {}) =>
        new Promise((resolve, reject) => {
          try {
            const fn = COMMANDS[cmd] || raise(`mock: unknown command ${cmd}`);
            resolve(fn(args));
          } catch (e) {
            reject(e);
          }
        }),
    },
    event: { listen: () => Promise.resolve(() => {}) },
  };
}
