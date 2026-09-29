// Capture: take a whole .env, picked, dropped or pasted, as the new
// latest snapshot of a project.
import { api } from '../api.js';
import { esc, debounce, plural, findProject } from '../util.js';
import { modalShell, folderField, projectNames } from '../kit.js';
import {
  S, $, run, busy, toast, openModal, renderModal, focusNext, loadProjects, selectProject,
} from '../app.js';

function openCaptureModal(fields) {
  const m = {
    kind: 'capture',
    tab: 'paste',
    picked: null,
    pastedText: '',
    projectName: '',
    pathHint: '',
    preview: null,
    busy: false,
    ...fields,
  };
  openModal(m, m.tab === 'paste' ? '#capture-text' : '#capture-project');
  refreshCapturePreview();
}

// A file from the picker or a drop: fills the open capture dialog, or opens one.
export function applyPickedFile(picked) {
  const m = S.modal;
  if (m?.kind !== 'capture') {
    openCaptureModal({ tab: 'file', picked, projectName: picked.nameGuess || '', pathHint: picked.dir || '' });
    return;
  }
  m.tab = 'file';
  m.picked = picked;
  m.projectName ||= picked.nameGuess || '';
  m.pathHint ||= picked.dir || '';
  focusNext('#capture-project');
  renderModal();
  refreshCapturePreview();
}

function captureText(m) {
  return m.tab === 'file' ? (m.picked?.text ?? '') : m.pastedText;
}

const refreshCapturePreview = debounce(async () => {
  const m = S.modal;
  if (m?.kind !== 'capture') return;
  const text = captureText(m);
  m.preview = text.trim() ? await api.previewCapture(text) : null;
  const box = $('#capture-preview');
  if (box && S.modal === m) box.innerHTML = capturePreviewView(m.preview);
}, 200);

// ------------------------------------------------------------------ views

function capturePreviewView(p) {
  if (!p) return '<span class="muted">Nothing to capture yet.</span>';
  const bits = [`<strong>${p.entries}</strong> ${p.entries === 1 ? 'entry' : 'entries'}`];
  if (p.comments) bits.push(plural(p.comments, 'comment', 'comments'));
  if (p.bad) bits.push(`<span class="warn">${plural(p.bad, 'line', 'lines')} not parsed</span>`);
  if (p.dupKeys.length) bits.push(`<span class="warn">duplicate: ${esc(p.dupKeys.join(', '))}</span>`);
  return bits.join(' <span class="dot">·</span> ');
}

function captureProjectHintView(S, m) {
  const name = m.projectName.trim();
  if (!name) return '<span class="muted">Give the snapshot a home.</span>';
  const existing = findProject(S.projects, name);
  if (!existing) return `Creates a new project <strong>${esc(name)}</strong>.`;
  const n = existing.entryCount;
  if (!n) return `Becomes the new latest snapshot of <strong>${esc(existing.name)}</strong>, which is empty right now.`;
  const tail =
    m.tab === 'paste'
      ? 'Paste the complete .env, not just what changed; the replaced snapshot stays in history.'
      : 'The project will show exactly what the file contains; the replaced snapshot stays in history.';
  return `<span class="warn">This replaces the current snapshot of <strong>${esc(existing.name)}</strong> — all ${plural(n, 'entry', 'entries')} — rather than adding to it. ${tail}</span>`;
}

function captureModal(S, m) {
  const fileTab = `
<div class="file-pick">
  <button class="btn" data-act="pick-file">Choose a .env file…</button>
  ${m.picked
    ? `<span class="file-path mono" title="${esc(m.picked.path)}">${esc(m.picked.path)}</span>`
    : '<span class="muted">or drop one anywhere in the window</span>'}
</div>`;
  const pasteTab = `
<textarea id="capture-text" data-input="field" data-field="pastedText" placeholder="# paste KEY=value lines…" spellcheck="false">${esc(m.pastedText)}</textarea>`;

  return modalShell({ cls: 'capture', label: 'Capture a snapshot' }, `
  <div class="seg">
    <button class="seg-btn${m.tab === 'file' ? ' on' : ''}" data-act="set" data-field="tab" data-value="file">From a file</button>
    <button class="seg-btn${m.tab === 'paste' ? ' on' : ''}" data-act="set" data-field="tab" data-value="paste">Paste</button>
  </div>
  ${m.tab === 'file' ? fileTab : pasteTab}
  <div class="field">
    <label for="capture-project">Project</label>
    <input id="capture-project" data-input="field" data-field="projectName" list="project-names" placeholder="Project name" value="${esc(m.projectName)}" autocomplete="off" spellcheck="false">
    ${projectNames(S.projects, 'project-names')}
    <p class="hint" id="capture-project-hint">${captureProjectHintView(S, m)}</p>
  </div>
  ${folderField('capture-hint', m.pathHint, 'data-input="field" data-field="pathHint"')}
  <div class="preview" id="capture-preview">${capturePreviewView(m.preview)}</div>
  <footer class="modal-foot">
    <span class="modal-note">A capture is the whole file at a point in time — earlier snapshots stay in history.</span>
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-accent" data-act="capture-submit"${m.busy ? ' disabled' : ''}>${m.busy ? 'Capturing…' : 'Capture'}</button>
  </footer>`);
}

// --------------------------------------------------------------- handlers

export const modals = {
  capture: {
    view: captureModal,
    changed: (m, field) => {
      if (field === 'projectName') {
        $('#capture-project-hint').innerHTML = captureProjectHintView(S, m);
      } else if (field === 'tab' || field === 'pastedText') {
        if (field === 'tab' && m.tab === 'paste') focusNext('#capture-text');
        refreshCapturePreview();
      }
    },
  },
};

export const actions = {
  'open-capture': () => openCaptureModal({}),
  recapture: () => openCaptureModal({ projectName: S.view.name, pathHint: S.view.pathHint || '' }),
  'pick-file': run(async () => {
    const picked = await api.pickEnvFile();
    if (picked) applyPickedFile(picked);
  }),
  'capture-submit': busy('capture', async (m) => {
    const text = captureText(m);
    const name = m.projectName.trim();
    if (!text.trim()) {
      toast('Nothing to capture — pick a file or paste some lines.', 'error');
      return;
    }
    if (!name) {
      toast('Give the project a name.', 'error');
      focusNext('#capture-project');
      return;
    }
    const existing = findProject(S.projects, name);
    const res = await api.capture({
      projectId: existing ? existing.id : null,
      projectName: existing ? null : name,
      pathHint: m.pathHint.trim() || null,
      text,
      sourcePath: m.tab === 'file' ? m.picked?.path ?? null : null,
    });
    S.modal = null;
    await loadProjects();
    await selectProject(res.projectId);
    toast(`Captured ${plural(res.entryCount, 'entry', 'entries')} into ${existing ? existing.name : name}`);
  }),
};

export const inputs = {};
export const forms = {};
