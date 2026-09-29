// Write .env.local: hand a snapshot's values to a .env.local in the
// project tree, either into a target file (merge or overwrite) or filled
// into a .env.example's layout. Rust stages each target behind an opaque
// token, and the token's staged kind decides which of the two it is;
// the webview only ever shows the path.
import { api } from '../api.js';
import { esc, errText } from '../util.js';
import { modalShell } from '../kit.js';
import { S, $, run, busy, toast, openModal, closeModal, renderModal } from '../app.js';

let previewSeq = 0;

// Recompute the write preview (counts and key names) without
// re-rendering the dialog, so tab and button focus survive. Only the
// latest request lands: a slow answer for a tab or mode the user has
// already left is dropped.
// The token for the open tab: the staged target, or the example scaffold.
const tabToken = (m) => (m.tab === 'example' ? m.example?.token : m.token);

async function refreshWritePreview(m) {
  const seq = ++previewSeq;
  const v = S.view;
  const token = tabToken(m);
  let preview;
  try {
    preview = token ? await api.previewWrite(v.id, v.snapshotId, token, m.mode === 'merge') : null;
  } catch (e) {
    preview = { blocked: errText(e), resultEntryCount: 0, added: [], substituted: [], emptied: [], kept: [] };
  }
  if (seq !== previewSeq || S.modal !== m) return;
  m.preview = preview;
  const box = $('#write-preview');
  if (box) box.innerHTML = writePreviewView(preview);
}

// ------------------------------------------------------------------ views

function writeClassBadge(cls) {
  if (cls === 'writable') return '<span class="tag tag-ok">writable</span>';
  return `<span class="tag tag-bad">${esc(cls === 'example' ? 'example file — blocked' : 'not .env.local — blocked')}</span>`;
}

// Counts and key chips for a write or merge preview. Names only, no values.
function writePreviewView(p) {
  if (!p) return '<span class="muted">…</span>';
  if (p.blocked) return `<span class="warn">${esc(p.blocked)}</span>`;
  const bits = [`<strong>${p.resultEntryCount}</strong> ${p.resultEntryCount === 1 ? 'entry' : 'entries'}`];
  if (p.substituted.length) bits.push(`updates ${p.substituted.length}`);
  if (p.kept.length) bits.push(`keeps ${p.kept.length}`);
  if (p.emptied.length) bits.push(`blanks ${p.emptied.length}`);
  if (p.added.length) bits.push(`adds ${p.added.length}`);
  return bits.join(' <span class="dot">·</span> ');
}

const segButton = (m, field, value, label) =>
  `<button class="seg-btn${m[field] === value ? ' on' : ''}" data-act="set" data-field="${field}" data-value="${value}">${label}</button>`;

const canWrite = (m, cls) => cls === 'writable' && !m.busy && !m.preview?.blocked;

function exampleBody(m) {
  const ex = m.example;
  const body = ex
    ? `
<p class="muted">Filling <span class="mono">${esc(ex.exampleName)}</span>’s comments and keys with this project’s values, written beside it:</p>
<p class="mono settings-path" title="${esc(ex.outPath)}">${esc(ex.outPath)} ${writeClassBadge(ex.outClass)}</p>
<div class="preview" id="write-preview">${writePreviewView(m.preview)}</div>`
    : `<p class="muted">Pick a <span class="mono">.env.example</span> for its <span class="mono">#</span> comments and key labels. Envarsa fills in this project’s values and writes a <span class="mono">.env.local</span> next to it — the example file is only read, never written.</p>`;
  return `
${body}
<footer class="modal-foot">
  <button class="btn" data-act="write-pick-example">${ex ? 'Choose a different example…' : 'Choose .env.example…'}</button>
  <span class="spacer"></span>
  <button class="btn" data-act="close-modal">Cancel</button>
  ${ex ? `<button class="btn btn-accent" data-act="write-confirm"${canWrite(m, ex.outClass) ? '' : ' disabled'}>${m.busy ? 'Writing…' : 'Write .env.local'}</button>` : ''}
</footer>`;
}

function targetBody(m) {
  if (!m.token) {
    return `
<p class="muted">This project has no remembered folder. Choose where to write its <span class="mono">.env.local</span> — Envarsa only writes to a <span class="mono">.env*.local</span>, never a committed example file.</p>
<footer class="modal-foot">
  <button class="btn btn-accent" data-act="write-change-location">Choose location…</button>
  <span class="spacer"></span>
  <button class="btn" data-act="close-modal">Cancel</button>
</footer>`;
  }
  const modeSeg = m.exists
    ? `
<div class="seg">
  ${segButton(m, 'mode', 'merge', 'Merge')}
  ${segButton(m, 'mode', 'overwrite', 'Overwrite')}
</div>
<p class="hint">${m.mode === 'merge'
        ? 'Keeps the file’s own comments and any keys it has that this project doesn’t; updates the rest.'
        : 'Replaces the file’s contents with this project’s current values.'}</p>`
    : '<p class="hint">This file doesn’t exist yet — it will be created.</p>';
  return `
<p class="mono settings-path" title="${esc(m.path)}">${esc(m.path)} ${writeClassBadge(m.class)}</p>
${modeSeg}
<div class="preview" id="write-preview">${writePreviewView(m.preview)}</div>
<footer class="modal-foot">
  <button class="btn" data-act="write-change-location">Change location…</button>
  <span class="spacer"></span>
  <button class="btn" data-act="close-modal">Cancel</button>
  <button class="btn btn-accent" data-act="write-confirm"${canWrite(m, m.class) ? '' : ' disabled'}>${m.busy ? 'Writing…' : 'Write'}</button>
</footer>`;
}

function writeModal(S, m) {
  return modalShell({ cls: 'write', label: 'Write .env.local' }, `
<div class="seg">
  ${segButton(m, 'tab', 'target', 'To a .env.local')}
  ${segButton(m, 'tab', 'example', 'From a .env.example')}
</div>
${m.tab === 'example' ? exampleBody(m) : targetBody(m)}`);
}

// --------------------------------------------------------------- handlers

export const modals = {
  write: {
    view: writeModal,
    // Each tab keeps its own staged target, so its preview starts afresh.
    changed: (m, field) => {
      if (field === 'tab') m.preview = null;
      refreshWritePreview(m);
    },
  },
};

export const actions = {
  'open-write': run(async () => {
    const v = S.view;
    // No remembered directory is fine: the dialog offers "Choose location".
    const target = await api.stageWriteTarget(v.id, v.snapshotId).catch(() => null);
    const m = {
      kind: 'write',
      tab: 'target',
      // Merge into an existing file by default; a new file is written fresh either way.
      mode: 'merge',
      token: null,
      path: null,
      class: null,
      exists: false,
      dir: null,
      example: null,
      preview: null,
      busy: false,
    };
    Object.assign(m, target);
    openModal(m);
    refreshWritePreview(m);
  }),
  'write-change-location': run(async () => {
    const m = S.modal;
    const t = await api.pickWriteTarget(m.dir || null);
    if (!t || S.modal !== m) return;
    Object.assign(m, t);
    renderModal();
    refreshWritePreview(m);
  }),
  'write-pick-example': run(async () => {
    const m = S.modal;
    const ex = await api.pickExampleFile();
    if (!ex || S.modal !== m) return;
    m.example = ex;
    renderModal();
    refreshWritePreview(m);
  }),
  'write-confirm': busy('write', async (m) => {
    const path = await api.writeEnvLocal(S.view.id, S.view.snapshotId, tabToken(m), m.mode === 'merge');
    closeModal();
    toast(m.tab === 'example' ? 'Wrote .env.local from the example' : 'Wrote .env.local', 'success', path);
  }),
};

export const inputs = {};
export const forms = {};
