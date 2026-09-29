// The library: the project rail, the snapshot sheet with its masked
// lines, reveal and copy, snapshot history, the reuse popover, and the
// edit and delete dialogs.
import { api } from '../api.js';
import { esc, timeAgo, fullTime, plural, errText } from '../util.js';
import { ICONS, brandMark, modalShell, folderField } from '../kit.js';
import {
  S, $, run, busy, toast, render, renderPopover, openModal, formError, clearRevealed,
  loadView, selectProject, refreshAfterMutation,
} from '../app.js';

const REVEAL_MS = 30_000;
const MASK = '<span class="mask" aria-label="hidden value">••••••••••</span>';

// ------------------------------------------------------------------ rail

function projectListView(S) {
  const q = S.filterProjects.trim().toLowerCase();
  const items = S.projects.filter(
    (p) =>
      !q ||
      p.name.toLowerCase().includes(q) ||
      (p.pathHint || '').toLowerCase().includes(q)
  );
  if (S.projects.length === 0) {
    return '<div class="rail-empty">Nothing captured yet.</div>';
  }
  if (items.length === 0) {
    return `<div class="rail-empty">No project matches “${esc(S.filterProjects)}”.</div>`;
  }
  return items
    .map((p) => {
      const active = p.id === S.selId ? ' active' : '';
      const shared = p.sharedKeys
        ? ` <span class="shared-chip" title="${p.sharedKeys} of these keys also live in other projects">${p.sharedKeys} shared</span>`
        : '';
      return `
<button class="project-item${active}" data-act="select-project" data-id="${esc(p.id)}">
  <span class="project-name">${esc(p.name)}</span>
  <span class="project-meta">${plural(p.entryCount, 'entry', 'entries')} · ${esc(timeAgo(p.latestCapturedAt))}${shared}</span>
</button>`;
    })
    .join('');
}

export function railView(S) {
  const enc = S.status?.encrypted;
  return `
<div class="rail-head">
  <div class="brand">${brandMark(24)}<span class="brand-name">Envarsa</span></div>
  <button class="btn btn-accent btn-block" data-act="open-capture"><span class="btn-ic">${ICONS.plus}</span>Capture</button>
  <button class="btn btn-block" data-act="open-editor-new" title="Create a project by hand — type variables and comments, kept in the store; no .env file needed"><span class="btn-ic">${ICONS.pencil}</span>New by hand</button>
</div>
<div class="rail-filter">
  <input id="filter-projects" type="search" placeholder="Filter projects" data-input="filter-projects" value="${esc(S.filterProjects)}" autocomplete="off" spellcheck="false">
</div>
<nav class="project-list" id="project-list">${projectListView(S)}</nav>
<div class="rail-foot">
  <button class="state-chip" data-act="open-settings" title="Store: ${esc(S.status?.storePath || '')}">
    <span class="state-ic">${enc ? ICONS.lock : ICONS.unlock}</span>${enc ? 'Encrypted' : 'Plaintext'}
  </button>
  <span class="spacer"></span>
  ${enc ? `<button class="icon-btn" data-act="lock" title="Lock the store">${ICONS.lock}</button>` : ''}
  <button class="icon-btn${S.status?.updateAvailable ? ' has-update' : ''}" data-act="open-settings" title="Settings${S.status?.updateAvailable ? ` — Envarsa ${esc(S.status.updateAvailable)} is available` : ''}">${ICONS.settings}</button>
</div>`;
}

// ----------------------------------------------------------------- sheet

function reuseBadge(line) {
  const n = line.reuse.length;
  if (!n) return '';
  const tone = line.reuse.some((r) => r.same) ? ' reuse-same' : '';
  return `<button class="reuse${tone}" data-act="reuse-badge" data-idx="${line.idx}" title="${esc(line.key)} also lives in ${plural(n, 'other project', 'other projects')}"><span class="reuse-ic">${ICONS.link}</span>${n}</button>`;
}

const revealButton = (idx, shown, what) =>
  `<button class="icon-btn" data-act="${shown ? 'hide' : 'reveal'}" data-idx="${idx}" title="${shown ? `Hide ${what}` : `Reveal ${what} (auto-hides after 30s)`}">${shown ? ICONS.eyeOff : ICONS.eye}</button>`;

// line type -> view(line, revealed?)
const LINE_VIEWS = {
  blank: () => '<div class="line line-blank"></div>',
  comment: (l) => `<div class="line line-comment mono">${esc(l.text)}</div>`,
  // A malformed line is as likely as any to hold a secret (a pasted
  // token, a header), so it is masked like a value.
  bad: (l, shown) => `
<div class="line line-bad${shown ? ' revealed' : ''}">
  <div class="cell-key">
    <span class="tag tag-bad" title="Kept in the snapshot byte-for-byte, but it isn't a KEY=value line — masked in case it holds a secret">not parsed</span>
  </div>
  <div class="cell-value">${shown ? `<span class="value-text mono">${esc(shown.value)}</span>` : MASK}</div>
  <div class="cell-actions">
    ${revealButton(l.idx, shown, 'line')}
  </div>
</div>`,
  entry: (l, shown) => `
<div class="line line-entry${l.overridden ? ' overridden' : ''}${shown ? ' revealed' : ''}">
  <div class="cell-key">
    <span class="key mono">${esc(l.key)}</span>
    ${l.exported ? '<span class="tag">export</span>' : ''}
    ${l.overridden ? '<span class="tag" title="A later line in this snapshot overrides this key">overridden</span>' : ''}
    ${reuseBadge(l)}
  </div>
  <div class="cell-value">${shown ? `<span class="value-text mono">${shown.value === '' ? '<span class="value-empty">empty</span>' : esc(shown.value)}</span>` : MASK}</div>
  <div class="cell-actions">
    ${revealButton(l.idx, shown, 'value')}
    <button class="icon-btn" data-act="copy-value" data-idx="${l.idx}" title="Copy value — straight to the clipboard (never shown), cleared after 30s">${ICONS.copy}</button>
  </div>
</div>`,
};

function rowsView(S) {
  const v = S.view;
  if (!v) return '';
  const q = S.filterKeys.trim().toLowerCase();
  let lines = v.lines;
  if (q) {
    lines = lines.filter((l) => l.t === 'entry' && l.key.toLowerCase().includes(q));
    if (lines.length === 0) {
      return `<div class="rows-empty">No key matches “${esc(S.filterKeys)}”.</div>`;
    }
  }
  if (v.lines.length === 0) {
    return '<div class="rows-empty">This snapshot is empty.</div>';
  }
  return lines.map((l) => LINE_VIEWS[l.t](l, S.revealed.get(l.idx))).join('');
}

function snapshotOptions(v) {
  return v.snapshots
    .map((s, i) => {
      const label = `${i === 0 ? 'Latest — ' : ''}${fullTime(s.capturedAt)} · ${plural(s.entryCount, 'entry', 'entries')} · ${s.via}`;
      const selected = s.id === v.snapshotId ? ' selected' : '';
      return `<option value="${esc(s.id)}"${selected}>${esc(label)}</option>`;
    })
    .join('');
}

function hideAllSlotView(S) {
  return S.revealed.size > 0
    ? '<button class="link-btn" data-act="hide-all">Hide revealed</button>'
    : '';
}

export function sheetView(S) {
  if (!S.status || S.status.state !== 'unlocked') return '';
  if (S.projects.length === 0) {
    return `
<div class="hero-empty">
  ${brandMark(64)}
  <h2>Your env values, under one roof</h2>
  <p>Capture a project's .env as a point-in-time snapshot. Envarsa keeps it durable, masked,<br>and ready to hand back — by copy or export — whenever you need it again.</p>
  <button class="btn btn-accent btn-lg" data-act="open-capture"><span class="btn-ic">${ICONS.plus}</span>Capture your first project</button>
  <p class="hero-hint">…or drop a .env file anywhere in this window, or <button class="link-btn" data-act="open-editor-new">build one by hand</button>.</p>
</div>`;
  }
  const v = S.view;
  if (!v) {
    return '<div class="hero-empty"><p class="hero-hint">Select a project on the left.</p></div>';
  }

  const oldBanner = !v.isLatest
    ? `
<div class="banner-old">
  <span>Viewing history — snapshot from <strong>${esc(fullTime(v.capturedAt))}</strong>.</span>
  <span class="spacer"></span>
  <button class="btn btn-sm" data-act="promote-snapshot">Bring this back as latest</button>
  <button class="btn btn-sm btn-ghost" data-act="back-to-latest">Back to latest</button>
</div>`
    : '';

  return `
<header class="sheet-head">
  <div class="title-row">
    <h1 class="sheet-title" title="${esc(v.name)}">${esc(v.name)}</h1>
    <button class="icon-btn" data-act="open-edit" title="Rename / edit filepath">${ICONS.pencil}</button>
    <span class="spacer"></span>
    <button class="btn" data-act="recapture" title="Capture a fresh snapshot to replace this one — the current snapshot stays in history"><span class="btn-ic">${ICONS.refresh}</span>Re-capture</button>
    <button class="btn" data-act="copy-block" title="Copy this snapshot to the clipboard, exactly as captured — cleared after 30s"><span class="btn-ic">${ICONS.copy}</span>Copy block</button>
    <button class="btn" data-act="open-editor" title="Edit this snapshot's variables — saves as a new snapshot; the current one stays in history"><span class="btn-ic">${ICONS.pencil}</span>Edit values</button>
    <button class="btn" data-act="export" title="Save this snapshot as a .env file — you choose where"><span class="btn-ic">${ICONS.download}</span>Export</button>
    <button class="btn" data-act="open-write" title="Write these values into a .env.local in your project (never a .env.example)"><span class="btn-ic">${ICONS.download}</span>Write .env.local</button>
    <button class="icon-btn danger" data-act="open-delete" title="Delete project from the library">${ICONS.trash}</button>
  </div>
  <div class="sub-row">
    ${v.pathHint ? `<span class="path mono" title="Filepath — a note, never a binding; the project's identity is its name">${esc(v.pathHint)}</span><span class="dot">·</span>` : ''}
    <span title="${esc(fullTime(v.capturedAt))}">captured ${esc(timeAgo(v.capturedAt))} via ${esc(v.via)}</span>
    <span class="dot">·</span>
    <span>${plural(v.entryCount, 'entry', 'entries')}</span>
    ${v.snapshots.length > 1 ? `<span class="dot">·</span><select class="snapshot-select" data-input="snapshot-select" title="Snapshot history">${snapshotOptions(v)}</select>` : ''}
  </div>
  ${oldBanner}
  <div class="tools-row">
    <input id="filter-keys" type="search" placeholder="Filter keys" data-input="filter-keys" value="${esc(S.filterKeys)}" autocomplete="off" spellcheck="false">
    <span id="hide-all-slot">${hideAllSlotView(S)}</span>
  </div>
</header>
<div class="entry-rows" id="entry-rows">${rowsView(S)}</div>`;
}

export function popoverView(S) {
  const p = S.popover;
  if (!p) return '';
  const rows = p.reuse
    .map(
      (r) => `
<button class="popover-row" data-act="goto-project" data-id="${esc(r.projectId)}">
  <span class="popover-name">${esc(r.name)}</span>
  <span class="pill ${r.same ? 'pill-same' : 'pill-diff'}">${r.same ? 'same value' : 'different value'}</span>
</button>`
    )
    .join('');
  return `
<div class="popover" style="left:${p.x}px; top:${p.y}px">
  <div class="popover-title mono">${esc(p.key)}</div>
  <div class="popover-sub">also lives in</div>
  ${rows}
</div>`;
}

// ---------------------------------------------------------------- dialogs

function editModal(S, m) {
  return modalShell({ cls: 'edit', label: 'Edit project' }, `
  <form data-form="edit-project" class="stack">
    <div class="field">
      <label for="edit-name">Name <span class="muted">(the project's identity)</span></label>
      <input id="edit-name" name="name" value="${esc(m.name)}" autocomplete="off" spellcheck="false">
    </div>
    ${folderField('edit-hint', m.pathHint, 'name="hint"')}
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Save</button>
    </footer>
  </form>`);
}

function deleteModal(S, m) {
  return modalShell({ cls: 'delete', label: 'Delete project', title: `Delete “${esc(m.name)}”?` }, `
  <p>Removes the project and its ${plural(m.snapshotCount, 'snapshot', 'snapshots')} from the library.</p>
  <p class="muted">The previous version of the store survives as <span class="mono">.bak</span> next to the store file until the next save.</p>
  <footer class="modal-foot">
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-danger" data-act="delete-confirm"${m.busy ? ' disabled' : ''}>${m.busy ? 'Deleting…' : 'Delete project'}</button>
  </footer>`);
}

// ---------------------------------------------------------------- reveal

// Rows plus the "Hide revealed" slot, without touching the filter input,
// so typing focus survives reveal changes such as the auto-hide.
function renderRowsAndTools() {
  const rows = $('#entry-rows');
  if (rows) rows.innerHTML = rowsView(S);
  const slot = $('#hide-all-slot');
  if (slot) slot.innerHTML = hideAllSlotView(S);
}

// Show a line's value for REVEAL_MS, or hide it (value null). Any timer
// already running for the line is cleared first, so a stale one can't
// mask a fresh reveal early.
function reveal(idx, value) {
  clearTimeout(S.revealed.get(idx)?.timer);
  if (value === null) {
    S.revealed.delete(idx);
  } else {
    S.revealed.set(idx, { value, timer: setTimeout(() => reveal(idx, null), REVEAL_MS) });
  }
  renderRowsAndTools();
}

export function hideRevealed() {
  if (!S.revealed.size) return;
  clearRevealed();
  renderRowsAndTools();
}

// -------------------------------------------------------------- handlers

export const modals = {
  edit: { view: editModal },
  delete: { view: deleteModal },
};

export const actions = {
  'select-project': run(async (d) => selectProject(d.id)),
  'goto-project': run(async (d) => {
    S.popover = null;
    renderPopover();
    await selectProject(d.id);
  }),

  reveal: run(async (d) => {
    const v = S.view;
    const idx = Number(d.idx);
    const r = await api.revealValue(v.id, v.snapshotId, idx);
    // Moved to another project or snapshot meanwhile: this idx is not that line.
    if (S.view === v) reveal(idx, r.value);
  }),
  hide: (d) => reveal(Number(d.idx), null),
  'hide-all': hideRevealed,
  'copy-value': run(async (d) => {
    const key = await api.copyValue(S.view.id, S.view.snapshotId, Number(d.idx));
    toast(`Copied ${key} — straight to the clipboard, cleared after 30s`);
  }),
  'copy-block': run(async () => {
    const n = await api.copyBlock(S.view.id, S.view.snapshotId);
    toast(`Copied the whole block — ${plural(n, 'entry', 'entries')}, exactly as captured; clears in 30s`);
  }),
  export: run(async () => {
    const path = await api.exportSnapshot(S.view.id, S.view.snapshotId);
    if (path) toast('Exported snapshot', 'success', path);
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

  'promote-snapshot': run(async () => {
    await api.promoteSnapshot(S.view.id, S.view.snapshotId);
    await refreshAfterMutation();
    toast('Snapshot restored as latest');
  }),
  'back-to-latest': run(async () => selectProject(S.selId)),

  'open-edit': () =>
    openModal({ kind: 'edit', name: S.view.name, pathHint: S.view.pathHint || '' }, '#edit-name'),
  'open-delete': () =>
    openModal({ kind: 'delete', name: S.view.name, snapshotCount: S.view.snapshots.length, busy: false }),
  'delete-confirm': busy('delete', async (m) => {
    await api.deleteProject(S.selId);
    S.modal = null;
    S.selId = null;
    localStorage.removeItem('envarsa.sel');
    await refreshAfterMutation();
    toast(`Deleted ${m.name} from the library`);
  }),
};

export const inputs = {
  'filter-projects': (value) => {
    S.filterProjects = value;
    $('#project-list').innerHTML = projectListView(S);
  },
  'filter-keys': (value) => {
    S.filterKeys = value;
    $('#entry-rows').innerHTML = rowsView(S);
  },
  'snapshot-select': run(async (snapshotId) => {
    await loadView(snapshotId);
    render();
  }),
};

export const forms = {
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
      formError(form, errText(e));
    }
  },
};
