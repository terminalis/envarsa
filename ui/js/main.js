// State, actions, and wiring. Rendering is delegated to views.js;
// IPC to api.js. One-way flow: action -> state -> render.
import { api, onEvent } from './api.js';
import { debounce } from './util.js';
import * as V from './views.js';
import { runSelftest } from './selftest.js';

const REVEAL_MS = 30_000;

const S = {
  status: null,
  projects: [],
  selId: null,
  view: null,
  revealed: new Map(), // idx -> { value, timer }
  filterProjects: '',
  filterKeys: '',
  modal: null,
  popover: null,
  toasts: [],
};

const $ = (sel) => document.querySelector(sel);

// ------------------------------------------------------------- rendering

let focusAfterRender = null;

function applyPendingFocus() {
  if (!focusAfterRender) return;
  const el = $(focusAfterRender);
  if (el) {
    el.focus();
    if (el.select) el.select();
  }
  focusAfterRender = null;
}

function render() {
  $('#overlay').innerHTML = V.gateView(S);
  const unlocked = S.status?.state === 'unlocked';
  $('#app').classList.toggle('gated', !unlocked);
  $('#rail').innerHTML = unlocked ? V.railView(S) : '';
  $('#sheet').innerHTML = unlocked ? V.sheetView(S) : '';
  renderModal();
  renderPopover();
  if (S.status?.state === 'locked') $('#unlock-pass')?.focus();
}

function renderModal() {
  $('#modal').innerHTML = V.modalView(S);
  applyPendingFocus();
}

function renderPopover() {
  $('#popover').innerHTML = V.popoverView(S);
}

function renderRows() {
  const rows = $('#entry-rows');
  if (rows) rows.innerHTML = V.rowsView(S);
}

// Rows + the "Hide revealed" slot, without touching the filter input
// (so typing focus survives reveal-state changes, e.g. auto-rehide).
function renderRowsAndTools() {
  renderRows();
  const slot = $('#hide-all-slot');
  if (slot) slot.innerHTML = V.hideAllSlotView(S);
}

function renderProjectList() {
  const list = $('#project-list');
  if (list) list.innerHTML = V.projectListView(S);
}

function renderToasts() {
  $('#toasts').innerHTML = V.toastsView(S.toasts);
}

function toast(msg, kind = 'success', detail = null) {
  const t = { msg, kind, detail };
  S.toasts.push(t);
  renderToasts();
  setTimeout(() => {
    S.toasts = S.toasts.filter((x) => x !== t);
    renderToasts();
  }, kind === 'error' ? 6000 : 3200);
}

const run = (fn) => async (...args) => {
  try {
    await fn(...args);
  } catch (err) {
    toast(String(err?.message || err), 'error');
  }
};

// ------------------------------------------------------------- data flow

function clearRevealed() {
  for (const r of S.revealed.values()) clearTimeout(r.timer);
  S.revealed.clear();
}

async function loadProjects() {
  S.projects = await api.listProjects();
  if (S.selId && !S.projects.some((p) => p.id === S.selId)) S.selId = null;
  if (!S.selId && S.projects.length) {
    const saved = localStorage.getItem('envarsa.sel');
    S.selId = S.projects.some((p) => p.id === saved) ? saved : S.projects[0].id;
  }
}

async function loadView(snapshotId = null) {
  clearRevealed();
  S.filterKeys = '';
  S.view = S.selId ? await api.getProject(S.selId, snapshotId) : null;
}

async function boot() {
  S.status = await api.status();
  S.modal = null;
  S.popover = null;
  clearRevealed();
  if (S.status.state === 'unlocked') {
    await loadProjects();
    await loadView();
  } else {
    S.projects = [];
    S.view = null;
    S.selId = null;
  }
  render();
}

async function refreshAfterMutation({ keepView = true } = {}) {
  await loadProjects();
  if (keepView && S.selId) {
    await loadView();
  } else {
    S.view = null;
  }
  render();
}

async function selectProject(id, snapshotId = null) {
  S.selId = id;
  localStorage.setItem('envarsa.sel', id);
  S.filterKeys = '';
  await loadView(snapshotId);
  render();
}

// --------------------------------------------------------------- capture

function openCaptureModal(preset = {}) {
  const tab = preset.picked ? 'file' : preset.tab || 'file';
  S.modal = {
    kind: 'capture',
    tab,
    picked: preset.picked || null,
    pastedText: preset.pastedText || '',
    projectName: preset.projectName || '',
    pathHint: preset.pathHint || '',
    preview: null,
    busy: false,
  };
  focusAfterRender = preset.picked ? '#capture-project' : tab === 'paste' ? '#capture-text' : null;
  renderModal();
  refreshCapturePreview();
}

function captureText(m) {
  return m.tab === 'file' ? (m.picked?.text ?? '') : m.pastedText;
}

const refreshCapturePreview = debounce(async () => {
  const m = S.modal;
  if (!m || m.kind !== 'capture') return;
  const text = captureText(m);
  m.preview = text.trim() ? await api.previewCapture(text) : null;
  const box = $('#capture-preview');
  if (box) box.innerHTML = V.capturePreviewView(m.preview);
}, 200);

function applyPickedFile(picked) {
  const m = S.modal;
  if (m?.kind === 'capture') {
    m.tab = 'file';
    m.picked = picked;
    if (!m.projectName && picked.nameGuess) m.projectName = picked.nameGuess;
    if (!m.pathHint && picked.dir) m.pathHint = picked.dir;
    focusAfterRender = '#capture-project';
    renderModal();
    refreshCapturePreview();
  } else {
    openCaptureModal({
      picked,
      projectName: picked.nameGuess || '',
      pathHint: picked.dir || '',
    });
  }
}

// ---------------------------------------------------------------- import

function suggestImportName(base, takenLower) {
  let candidate = `${base} (imported)`;
  let n = 2;
  while (takenLower.has(candidate.trim().toLowerCase())) {
    candidate = `${base} (imported ${n++})`;
  }
  takenLower.add(candidate.trim().toLowerCase());
  return candidate;
}

function openImportModal(preview, passphrase = null) {
  // Names already spoken for: every existing project plus the incoming
  // conflict-free ones — rename suggestions must dodge them all.
  const taken = new Set(S.projects.map((p) => p.name.trim().toLowerCase()));
  for (const p of preview.projects) {
    if (!p.conflictsWith) taken.add(p.name.trim().toLowerCase());
  }
  const decisions = preview.projects.map((p) =>
    p.conflictsWith
      ? { action: 'rename', newName: suggestImportName(p.name.trim(), taken) }
      : { action: 'add', newName: '' }
  );
  S.modal = {
    kind: 'import',
    token: preview.token,
    path: preview.path, // display only — apply sends the token back
    encrypted: preview.encrypted,
    unlocked: preview.unlocked,
    passphrase,
    projects: preview.projects,
    decisions,
    busy: false,
  };
  if (!preview.unlocked) focusAfterRender = '#import-pass';
  renderModal();
}

// Every name a row's rename suggestion must dodge: existing projects
// plus the final names every other row is currently set to take.
function importTakenNames(m, exceptIdx) {
  const taken = new Set(S.projects.map((p) => p.name.trim().toLowerCase()));
  m.projects.forEach((p, j) => {
    if (j === exceptIdx) return;
    const d = m.decisions[j];
    if (d.action === 'skip') return;
    const final = d.action === 'rename' ? (d.newName || '').trim() : p.name.trim();
    if (final) taken.add(final.toLowerCase());
  });
  return taken;
}

// Re-derive row hints, the summary line, and the Import button without
// re-rendering the modal — so typing a new name keeps its focus.
function updateImportDerived() {
  const m = S.modal;
  if (m?.kind !== 'import' || !m.unlocked) return;
  const plan = V.importPlan(S, m);
  m.projects.forEach((p, i) => {
    const hint = $(`#import-row-hint-${i}`);
    if (hint) {
      hint.innerHTML = plan.rows[i].note;
      hint.classList.toggle('warn', !plan.rows[i].ok);
    }
  });
  const summary = $('#import-summary');
  if (summary) summary.textContent = V.importSummaryText(plan);
  const btn = $('#import-apply-btn');
  if (btn) btn.disabled = m.busy || plan.problems > 0 || plan.importing === 0;
}

// ----------------------------------------------------------- write .env.local

// Recompute the write/merge preview (counts + key chips) without
// re-rendering the modal, so seg/tab focus survives.
async function refreshWritePreview() {
  const m = S.modal;
  if (m?.kind !== 'write') return;
  const v = S.view;
  try {
    if (m.tab === 'example') {
      m.preview = m.example
        ? await api.previewExampleWrite(v.id, v.snapshotId, m.example.token)
        : null;
    } else {
      m.preview = m.token ? await api.previewWrite(v.id, v.snapshotId, m.token, m.mode) : null;
    }
  } catch (e) {
    m.preview = {
      blocked: String(e?.message || e),
      resultEntryCount: 0,
      added: [],
      substituted: [],
      emptied: [],
      kept: [],
    };
  }
  if (S.modal !== m) return;
  const box = $('#write-preview');
  if (box) box.innerHTML = V.writePreviewView(m.preview);
}

// --------------------------------------------------------------- actions

const ACTIONS = {
  'select-project': run(async (d) => selectProject(d.id)),
  'goto-project': run(async (d) => {
    S.popover = null;
    renderPopover();
    await selectProject(d.id);
  }),

  'open-capture': () => openCaptureModal({ tab: 'paste' }),
  recapture: () => {
    const v = S.view;
    if (!v) return;
    openCaptureModal({
      tab: 'paste',
      projectName: v.name,
      pathHint: v.pathHint || '',
    });
  },
  'pick-file': run(async () => {
    const picked = await api.pickEnvFile();
    if (picked) applyPickedFile(picked);
  }),
  'tab-file': () => {
    S.modal.tab = 'file';
    renderModal();
    refreshCapturePreview();
  },
  'tab-paste': () => {
    S.modal.tab = 'paste';
    focusAfterRender = '#capture-text';
    renderModal();
    refreshCapturePreview();
  },
  'capture-submit': run(async () => {
    const m = S.modal;
    const text = captureText(m);
    const name = (m.projectName || '').trim();
    if (!text.trim()) {
      toast('Nothing to capture — pick a file or paste some lines.', 'error');
      return;
    }
    if (!name) {
      toast('Give the project a name.', 'error');
      focusAfterRender = '#capture-project';
      renderModal();
      return;
    }
    m.busy = true;
    renderModal();
    try {
      const existing = S.projects.find(
        (p) => p.name.trim().toLowerCase() === name.toLowerCase()
      );
      const res = await api.capture({
        projectId: existing ? existing.id : null,
        projectName: existing ? null : name,
        pathHint: (m.pathHint || '').trim() || null,
        text,
        sourcePath: m.tab === 'file' ? m.picked?.path ?? null : null,
      });
      S.modal = null;
      await loadProjects();
      await selectProject(res.projectId);
      toast(
        `Captured ${res.entryCount} ${res.entryCount === 1 ? 'entry' : 'entries'} into ${existing ? existing.name : name}`
      );
    } finally {
      if (S.modal?.kind === 'capture') {
        S.modal.busy = false;
        renderModal();
      }
    }
  }),

  reveal: run(async (d) => {
    const idx = Number(d.idx);
    const r = await api.revealValue(S.view.id, S.view.snapshotId, idx);
    const timer = setTimeout(() => {
      S.revealed.delete(idx);
      renderRowsAndTools();
    }, REVEAL_MS);
    S.revealed.set(idx, { value: r.value, timer });
    renderRowsAndTools();
  }),
  hide: (d) => {
    const idx = Number(d.idx);
    const r = S.revealed.get(idx);
    if (r) clearTimeout(r.timer);
    S.revealed.delete(idx);
    renderRowsAndTools();
  },
  'hide-all': () => {
    clearRevealed();
    render();
  },
  'copy-value': run(async (d) => {
    const key = await api.copyValue(S.view.id, S.view.snapshotId, Number(d.idx));
    toast(`Copied ${key} — straight to the clipboard, cleared after 30s`);
  }),
  'copy-block': run(async () => {
    const n = await api.copyBlock(S.view.id, S.view.snapshotId);
    toast(`Copied the whole block — ${n} ${n === 1 ? 'entry' : 'entries'}, exactly as captured; clears in 30s`);
  }),
  export: run(async () => {
    const path = await api.exportSnapshot(S.view.id, S.view.snapshotId);
    if (path) toast('Exported snapshot', 'success', path);
  }),

  'open-write': run(async () => {
    const v = S.view;
    if (!v) return;
    let t = null;
    // No remembered directory is fine — the modal offers "Change location".
    try {
      t = await api.stageWriteTarget(v.id, v.snapshotId);
    } catch {
      t = null;
    }
    S.modal = {
      kind: 'write',
      tab: 'target',
      token: t?.token || null,
      path: t?.path || null,
      class: t?.class || null,
      exists: t?.exists || false,
      dir: t?.dir || null,
      mode: t?.exists ? 'merge' : 'fresh',
      preview: null,
      busy: false,
      example: null,
    };
    renderModal();
    if (t) refreshWritePreview();
  }),
  'write-tab-target': () => {
    S.modal.tab = 'target';
    renderModal();
    if (S.modal.token) refreshWritePreview();
  },
  'write-tab-example': () => {
    S.modal.tab = 'example';
    renderModal();
    if (S.modal.example) refreshWritePreview();
  },
  'write-mode-merge': () => {
    S.modal.mode = 'merge';
    renderModal();
    refreshWritePreview();
  },
  'write-mode-overwrite': () => {
    S.modal.mode = 'overwrite';
    renderModal();
    refreshWritePreview();
  },
  'write-change-location': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'write') return;
    const t = await api.pickWriteTarget(m.dir || null);
    if (!t) return;
    m.token = t.token;
    m.path = t.path;
    m.class = t.class;
    m.exists = t.exists;
    m.dir = t.dir;
    if (!t.exists) m.mode = 'fresh';
    else if (m.mode === 'fresh') m.mode = 'merge';
    renderModal();
    refreshWritePreview();
  }),
  'write-pick-example': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'write') return;
    const ex = await api.pickExampleFile();
    if (!ex) return;
    m.example = ex;
    renderModal();
    refreshWritePreview();
  }),
  'write-confirm': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'write' || m.busy || !m.token) return;
    m.busy = true;
    renderModal();
    try {
      const path = await api.writeEnvLocal(S.view.id, S.view.snapshotId, m.token, m.mode);
      S.modal = null;
      renderModal();
      toast('Wrote .env.local', 'success', path);
    } finally {
      if (S.modal?.kind === 'write') {
        S.modal.busy = false;
        renderModal();
      }
    }
  }),
  'write-example-confirm': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'write' || m.busy || !m.example) return;
    m.busy = true;
    renderModal();
    try {
      const path = await api.writeExampleScaffold(S.view.id, S.view.snapshotId, m.example.token);
      S.modal = null;
      renderModal();
      toast('Wrote .env.local from the example', 'success', path);
    } finally {
      if (S.modal?.kind === 'write') {
        S.modal.busy = false;
        renderModal();
      }
    }
  }),

  'open-editor': run(async () => {
    const v = S.view;
    if (!v) return;
    const rows = await api.editLines(v.id, v.snapshotId);
    S.modal = {
      kind: 'editor',
      isNew: false,
      projectId: v.id,
      projectName: v.name,
      pathHint: v.pathHint || '',
      rows,
      busy: false,
    };
    renderModal();
  }),
  'open-editor-new': () => {
    S.modal = {
      kind: 'editor',
      isNew: true,
      projectId: null,
      projectName: '',
      pathHint: '',
      rows: [{ kind: 'entry', key: '', value: '', exported: false }],
      busy: false,
    };
    focusAfterRender = '#editor-name';
    renderModal();
  },
  'editor-add-entry': () => {
    S.modal.rows.push({ kind: 'entry', key: '', value: '', exported: false });
    renderModal();
  },
  'editor-add-comment': () => {
    S.modal.rows.push({ kind: 'comment', text: '# ' });
    renderModal();
  },
  'editor-del-row': (d) => {
    S.modal.rows.splice(Number(d.idx), 1);
    renderModal();
  },
  'editor-seed-example': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'editor') return;
    const ex = await api.pickExampleFile();
    if (!ex) return;
    const existing = new Set(m.rows.filter((r) => r.kind === 'entry').map((r) => r.key));
    const added = ex.exampleKeys
      .filter((k) => !existing.has(k))
      .map((k) => ({ kind: 'entry', key: k, value: '', exported: false }));
    if (!added.length) {
      toast('Those keys are already here');
      return;
    }
    m.rows.push({ kind: 'comment', text: `# from ${ex.exampleName}` }, ...added);
    renderModal();
    toast(`Added ${added.length} ${added.length === 1 ? 'key' : 'keys'} from ${ex.exampleName}`);
  }),
  'editor-save': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'editor' || m.busy) return;
    if (m.isNew && !(m.projectName || '').trim()) {
      toast('Give the project a name.', 'error');
      focusAfterRender = '#editor-name';
      renderModal();
      return;
    }
    m.busy = true;
    renderModal();
    try {
      const res = await api.saveEditedSnapshot({
        projectId: m.projectId,
        projectName: m.isNew ? m.projectName.trim() : null,
        pathHint: m.isNew ? (m.pathHint || '').trim() || null : null,
        lines: m.rows,
      });
      S.modal = null;
      await loadProjects();
      await selectProject(res.projectId);
      toast(`Saved ${res.entryCount} ${res.entryCount === 1 ? 'entry' : 'entries'}`);
    } finally {
      if (S.modal?.kind === 'editor') {
        S.modal.busy = false;
        renderModal();
      }
    }
  }),

  'reuse-badge': (d, btn) => {
    const idx = Number(d.idx);
    const line = S.view.lines.find((l) => l.t === 'entry' && l.idx === idx);
    if (!line) return;
    const rect = btn.getBoundingClientRect();
    const x = Math.min(rect.left, window.innerWidth - 280);
    const estHeight = 70 + line.reuse.length * 34;
    const y =
      rect.bottom + 6 + estHeight > window.innerHeight
        ? Math.max(8, rect.top - 6 - estHeight)
        : rect.bottom + 6;
    S.popover = { x, y, key: line.key, reuse: line.reuse };
    renderPopover();
  },

  'snapshot-select': () => {}, // handled as input/change
  'promote-snapshot': run(async () => {
    await api.promoteSnapshot(S.view.id, S.view.snapshotId);
    await refreshAfterMutation();
    toast('Snapshot restored as latest');
  }),
  'back-to-latest': run(async () => selectProject(S.selId)),

  'open-edit': () => {
    S.modal = {
      kind: 'edit',
      name: S.view.name,
      pathHint: S.view.pathHint || '',
    };
    focusAfterRender = '#edit-name';
    renderModal();
  },
  'open-delete': () => {
    S.modal = {
      kind: 'delete',
      name: S.view.name,
      snapshotCount: S.view.snapshots.length,
      busy: false,
    };
    renderModal();
  },
  'delete-confirm': run(async () => {
    S.modal.busy = true;
    renderModal();
    const name = S.modal.name;
    await api.deleteProject(S.selId);
    S.modal = null;
    S.selId = null;
    localStorage.removeItem('envarsa.sel');
    await refreshAfterMutation({ keepView: false });
    if (S.selId) await loadView();
    render();
    toast(`Deleted ${name} from the library`);
  }),

  lock: run(async () => {
    await api.lock();
    await boot();
  }),

  'open-settings': run(async () => {
    S.status = await api.status();
    S.modal = { kind: 'settings', updChecking: false, updDone: false, updError: null };
    renderModal();
  }),
  // Not run()-wrapped: a failed manual check reports inline in the
  // settings modal, not as a toast.
  'check-updates': async () => {
    const m = S.modal;
    if (m?.kind !== 'settings' || m.updChecking) return;
    m.updChecking = true;
    m.updError = null;
    m.updDone = false;
    renderModal();
    try {
      const res = await api.checkForUpdates();
      if (S.status) S.status.updateAvailable = res.updateAvailable ? res.latest : null;
      m.updDone = true;
    } catch (e) {
      m.updError = String(e?.message || e);
    } finally {
      if (S.modal?.kind === 'settings') {
        S.modal.updChecking = false;
        renderModal();
      }
      applyUpdateBadge();
    }
  },
  'open-releases': run(async () => api.openReleasesPage()),
  'reveal-store': run(async () => api.revealStore()),
  'relocate-store': run(async () => {
    const path = await api.relocateStore();
    if (path) {
      S.status = await api.status();
      render(); // settings modal re-renders with the new path
      toast('Store moved', 'success', path);
    }
  }),
  'open-export-store': () => {
    // Default to encrypting the copy when the store itself is encrypted.
    S.modal = { kind: 'export-store', encrypt: !!S.status?.encrypted };
    focusAfterRender = S.status?.encrypted ? '#export-pass' : null;
    renderModal();
  },
  'open-import-store': run(async () => {
    const token = await api.pickImportStore();
    if (!token) return;
    openImportModal(await api.inspectImport(token));
  }),
  'import-apply': run(async () => {
    const m = S.modal;
    if (m?.kind !== 'import' || m.busy) return;
    const plan = V.importPlan(S, m);
    if (plan.problems > 0 || plan.importing === 0) return;
    m.busy = true;
    renderModal();
    try {
      const decisions = m.projects.map((p, i) => ({
        name: p.name,
        action: m.decisions[i].action,
        newName: m.decisions[i].action === 'rename' ? m.decisions[i].newName.trim() : null,
      }));
      const sum = await api.applyImport(m.token, m.passphrase, decisions);
      S.modal = null;
      await refreshAfterMutation();
      const total = sum.added + sum.replaced + sum.renamed;
      const bits = [];
      if (sum.added) bits.push(`${sum.added} added`);
      if (sum.renamed) bits.push(`${sum.renamed} renamed`);
      if (sum.replaced) bits.push(`${sum.replaced} replaced`);
      if (sum.skipped) bits.push(`${sum.skipped} skipped`);
      toast(
        `Imported ${total} ${total === 1 ? 'project' : 'projects'} into the library`,
        'success',
        bits.join(' · ')
      );
    } finally {
      if (S.modal?.kind === 'import') {
        S.modal.busy = false;
        renderModal();
      }
    }
  }),
  'restore-backup': run(async () => {
    await api.restoreBackup();
    await boot();
    toast('Backup restored');
  }),
  'retry-boot': run(async () => boot()),

  'close-modal': () => {
    if (S.modal?.busy) return;
    S.modal = null;
    renderModal();
  },
};

// ---------------------------------------------------------------- inputs

const INPUTS = {
  'filter-projects': (value) => {
    S.filterProjects = value;
    renderProjectList();
  },
  'filter-keys': (value) => {
    S.filterKeys = value;
    renderRows();
  },
  'capture-text': (value) => {
    if (S.modal?.kind === 'capture') {
      S.modal.pastedText = value;
      refreshCapturePreview();
    }
  },
  'capture-project': (value) => {
    if (S.modal?.kind === 'capture') {
      S.modal.projectName = value;
      const hint = $('#capture-project-hint');
      if (hint) hint.innerHTML = V.captureProjectHintView(S, S.modal);
    }
  },
  'capture-hint': (value) => {
    if (S.modal?.kind === 'capture') S.modal.pathHint = value;
  },
  'export-encrypt': (value, el) => {
    if (S.modal?.kind !== 'export-store') return;
    S.modal.encrypt = el.checked;
    if (el.checked) focusAfterRender = '#export-pass';
    renderModal();
  },
  'auto-update-toggle': async (value, el) => {
    if (S.modal?.kind !== 'settings') return;
    try {
      await api.setAutoUpdateCheck(el.checked);
      if (S.status) S.status.autoUpdateCheck = el.checked;
    } catch (e) {
      el.checked = !el.checked;
      toast(String(e?.message || e), 'error');
    }
  },
  'import-action': (value, el) => {
    const m = S.modal;
    if (m?.kind !== 'import') return;
    const i = Number(el.dataset.idx);
    m.decisions[i].action = value;
    if (value === 'rename') {
      if (!m.decisions[i].newName) {
        m.decisions[i].newName = suggestImportName(
          m.projects[i].name.trim(),
          importTakenNames(m, i)
        );
      }
      focusAfterRender = `#import-rename-${i}`;
    }
    renderModal();
  },
  'import-rename': (value, el) => {
    const m = S.modal;
    if (m?.kind !== 'import') return;
    m.decisions[Number(el.dataset.idx)].newName = value;
    updateImportDerived();
  },
  // Editor rows mutate in place — no re-render, so typing focus survives.
  'editor-key': (value, el) => {
    if (S.modal?.kind === 'editor') S.modal.rows[Number(el.dataset.idx)].key = value;
  },
  'editor-value': (value, el) => {
    if (S.modal?.kind === 'editor') S.modal.rows[Number(el.dataset.idx)].value = value;
  },
  'editor-comment': (value, el) => {
    if (S.modal?.kind === 'editor') S.modal.rows[Number(el.dataset.idx)].text = value;
  },
  'editor-export': (value, el) => {
    if (S.modal?.kind === 'editor') S.modal.rows[Number(el.dataset.idx)].exported = el.checked;
  },
  'editor-name': (value) => {
    if (S.modal?.kind === 'editor') S.modal.projectName = value;
  },
  'editor-hint': (value) => {
    if (S.modal?.kind === 'editor') S.modal.pathHint = value;
  },
  'snapshot-select': (value) => {
    ACTIONS['_select-snapshot'](value);
  },
};

ACTIONS['_select-snapshot'] = run(async (snapshotId) => {
  await loadView(snapshotId);
  render();
});

// ----------------------------------------------------------------- forms

const formError = (form, msg) => {
  const el = form.querySelector('.form-error') || $('#unlock-error');
  if (el) el.textContent = msg || '';
};

const FORMS = {
  unlock: async (form) => {
    const pass = $('#unlock-pass').value;
    formError(form, '');
    try {
      await api.unlock(pass);
      await boot();
    } catch (e) {
      formError(form, String(e));
      $('#unlock-pass').select();
    }
  },
  'enc-enable': async (form) => {
    const f = new FormData(form);
    formError(form, '');
    if (String(f.get('pass')).length < 8) return formError(form, 'Use at least 8 characters.');
    if (f.get('pass') !== f.get('confirm')) return formError(form, 'The passphrases do not match.');
    if (!f.get('ack')) return formError(form, 'Tick the box — this is the one promise Envarsa cannot break for you.');
    try {
      await api.enableEncryption(String(f.get('pass')));
      S.status = await api.status();
      renderModal();
      render();
      toast('Store encrypted — keep that passphrase safe');
    } catch (e) {
      formError(form, String(e));
    }
  },
  'enc-change': async (form) => {
    const f = new FormData(form);
    formError(form, '');
    if (String(f.get('next')).length < 8) return formError(form, 'Use at least 8 characters.');
    if (f.get('next') !== f.get('confirm')) return formError(form, 'The new passphrases do not match.');
    try {
      await api.changePassphrase(String(f.get('current')), String(f.get('next')));
      renderModal();
      toast('Passphrase changed');
    } catch (e) {
      formError(form, String(e));
    }
  },
  'enc-disable': async (form) => {
    const f = new FormData(form);
    formError(form, '');
    try {
      await api.disableEncryption(String(f.get('current')));
      S.status = await api.status();
      renderModal();
      render();
      toast('Store decrypted — plaintext JSON on disk again');
    } catch (e) {
      formError(form, String(e));
    }
  },
  'export-store': async (form) => {
    const m = S.modal;
    if (m?.kind !== 'export-store') return;
    const f = new FormData(form);
    formError(form, '');
    let passphrase = null;
    if (m.encrypt) {
      passphrase = String(f.get('pass') || '');
      if (passphrase.length < 8) return formError(form, 'Use at least 8 characters.');
      if (passphrase !== f.get('confirm')) return formError(form, 'The passphrases do not match.');
    }
    // No re-render around the await: the typed passphrase stays put if
    // the user cancels the save dialog or the write fails.
    const btn = form.querySelector('button[type="submit"]');
    if (btn) {
      btn.disabled = true;
      btn.textContent = 'Exporting…';
    }
    try {
      const path = await api.exportStore(passphrase);
      if (path) {
        S.modal = null;
        renderModal();
        toast('Exported a copy of the store', 'success', path);
      }
    } catch (e) {
      formError(form, String(e));
    } finally {
      if (btn?.isConnected) {
        btn.disabled = false;
        btn.textContent = 'Choose where to save…';
      }
    }
  },
  'import-unlock': async (form) => {
    const m = S.modal;
    if (m?.kind !== 'import') return;
    const pass = String(new FormData(form).get('pass') || '');
    formError(form, '');
    if (!pass) return formError(form, 'Enter the passphrase.');
    const btn = form.querySelector('button[type="submit"]');
    if (btn) {
      btn.disabled = true;
      btn.textContent = 'Unlocking…';
    }
    try {
      // Decrypting re-runs scrypt — a second or two is normal.
      openImportModal(await api.inspectImport(m.token, pass), pass);
    } catch (e) {
      formError(form, String(e));
      if (btn?.isConnected) {
        btn.disabled = false;
        btn.textContent = 'Unlock';
      }
    }
  },
  'edit-project': async (form) => {
    const f = new FormData(form);
    formError(form, '');
    try {
      const name = String(f.get('name'));
      if (name.trim() !== S.view.name) await api.renameProject(S.selId, name);
      const hint = String(f.get('hint'));
      if (hint.trim() !== (S.view.pathHint || '')) await api.setPathHint(S.selId, hint);
      S.modal = null;
      await refreshAfterMutation();
      toast('Project updated');
    } catch (e) {
      formError(form, String(e));
    }
  },
};

// ----------------------------------------------------------------- wiring

document.addEventListener('click', (e) => {
  if (S.popover && !e.target.closest('.popover') && !e.target.closest('[data-act="reuse-badge"]')) {
    S.popover = null;
    renderPopover();
  }
  const el = e.target.closest('[data-act]');
  if (!el) return;
  const handler = ACTIONS[el.dataset.act];
  if (handler) handler(el.dataset, el, e);
});

document.addEventListener('input', (e) => {
  const key = e.target?.dataset?.input;
  if (key && key !== 'snapshot-select') INPUTS[key]?.(e.target.value, e.target);
});

document.addEventListener('change', (e) => {
  const key = e.target?.dataset?.input;
  if (key === 'snapshot-select') INPUTS[key]?.(e.target.value, e.target);
});

document.addEventListener('submit', (e) => {
  const kind = e.target?.dataset?.form;
  if (!kind) return;
  e.preventDefault();
  FORMS[kind]?.(e.target);
});

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    if (S.popover) {
      S.popover = null;
      renderPopover();
    } else if (S.modal && !S.modal.busy) {
      S.modal = null;
      renderModal();
    }
  }
});

// Privacy reflex: anything revealed hides when the window loses focus.
window.addEventListener('blur', () => {
  if (S.revealed.size) {
    clearRevealed();
    renderRowsAndTools();
  }
});

// Drag a .env anywhere in the window to capture it. The drop itself is
// handled in Rust (the path never enters the webview); what arrives
// here is a finished payload, ready to capture.
const dropzone = () => $('#dropzone');
onEvent('tauri://drag-enter', () => {
  if (S.status?.state === 'unlocked') dropzone().hidden = false;
});
onEvent('tauri://drag-leave', () => {
  dropzone().hidden = true;
});
onEvent('tauri://drag-drop', () => {
  dropzone().hidden = true;
});
onEvent('env-file-dropped', (e) => {
  if (S.status?.state !== 'unlocked') return;
  if (e?.payload) applyPickedFile(e.payload);
});
onEvent('env-drop-error', (e) => {
  toast(String(e?.payload || 'could not read the dropped file'), 'error');
});

// The background auto-check (opt-in) found a newer release. classList
// only — no innerHTML, so a check landing ~3s after boot can never
// steal typing focus from the rail filter.
function applyUpdateBadge() {
  document.querySelectorAll('[data-act="open-settings"].icon-btn').forEach((b) =>
    b.classList.toggle('has-update', !!S.status?.updateAvailable));
}
onEvent('update-available', (e) => {
  if (typeof e?.payload !== 'string' || !e.payload) return;
  if (S.status) S.status.updateAvailable = e.payload;
  applyUpdateBadge();
  if (S.modal?.kind === 'settings') renderModal();
});

window.addEventListener('error', (e) =>
  api.uiLog('error', `${e.message} @ ${e.filename}:${e.lineno}`)
);
window.addEventListener('unhandledrejection', (e) =>
  api.uiLog('error', `unhandled rejection: ${e.reason}`)
);

// ------------------------------------------------------------------ boot

(async function start() {
  try {
    await boot();
    api.uiLog('info', `ui ready — state=${S.status?.state}, projects=${S.projects.length}`);
    if (await api.selftest.enabled()) {
      api.uiLog('info', 'selftest starting');
      await runSelftest();
    }
  } catch (err) {
    api.uiLog('error', `boot failed: ${err?.message || err}`);
    document.body.innerHTML = `<div class="gate"><div class="gate-card"><h1>Envarsa</h1><p class="form-error">Boot failed: ${String(err?.message || err)}</p></div></div>`;
  }
})();
