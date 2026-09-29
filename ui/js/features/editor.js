// The structured editor: change a snapshot's lines, or build a project
// by hand, and save the result as a new snapshot.
import { api } from '../api.js';
import { esc, plural } from '../util.js';
import { ICONS, modalShell, folderField, projectNames } from '../kit.js';
import {
  S, run, busy, toast, openModal, renderModal, focusNext, loadProjects, selectProject,
} from '../app.js';

const newEntry = () => ({ kind: 'entry', key: '', value: '', exported: false });

// ------------------------------------------------------------------ views

const rowInput = (field, i, value, attrs) =>
  `<input ${attrs} data-input="field" data-field="${field}" data-idx="${i}" value="${esc(value)}" autocomplete="off" spellcheck="false">`;

// row kind -> view(row, idx, removeButton)
const ROW_VIEWS = {
  comment: (r, i, del) => `
<div class="editor-row editor-comment">
  ${rowInput('text', i, r.text, 'class="mono" placeholder="# comment"')}
  ${del}
</div>`,
  bad: (r, i, del) => `
<div class="editor-row editor-bad">
  <span class="mono" title="Kept verbatim — not a KEY=value line">${esc(r.raw)}</span>
  ${del}
</div>`,
  blank: (r, i, del) => `<div class="editor-row editor-blank"><span class="muted">— blank line —</span>${del}</div>`,
  entry: (r, i, del) => `
<div class="editor-row editor-entry">
  ${rowInput('key', i, r.key, 'class="mono editor-key" placeholder="KEY"')}
  ${rowInput('value', i, r.value, 'class="mono editor-value" placeholder="value"')}
  <label class="check editor-export" title="Write with an export prefix"><input type="checkbox" data-input="field" data-field="exported" data-idx="${i}"${r.exported ? ' checked' : ''}>export</label>
  ${del}
</div>`,
};

function editorModal(S, m) {
  const nameField = m.isNew
    ? `
<div class="field">
  <label for="editor-name">Project</label>
  <input id="editor-name" data-input="field" data-field="projectName" list="editor-project-names" value="${esc(m.projectName)}" placeholder="Project name" autocomplete="off" spellcheck="false">
  ${projectNames(S.projects, 'editor-project-names')}
</div>
${folderField('editor-hint', m.pathHint, 'data-input="field" data-field="pathHint"')}`
    : `<p class="muted">Editing <strong>${esc(m.projectName)}</strong> — saving adds a new snapshot; the current one stays in history.</p>`;

  const rows = m.rows
    .map((r, i) => ROW_VIEWS[r.kind](r, i, `<button class="icon-btn" data-act="editor-del-row" data-idx="${i}" title="Remove line">${ICONS.x}</button>`))
    .join('') || '<p class="muted editor-empty">No lines yet — add a variable or comment below.</p>';

  return modalShell({ cls: 'editor', label: 'Edit variables', title: m.isNew ? 'New project by hand' : 'Edit values' }, `
  ${nameField}
  <div class="editor-rows">${rows}</div>
  <div class="editor-tools">
    <button class="btn btn-sm" data-act="editor-add-entry"><span class="btn-ic">${ICONS.plus}</span>Variable</button>
    <button class="btn btn-sm" data-act="editor-add-comment"><span class="btn-ic">${ICONS.plus}</span>Comment</button>
    <button class="btn btn-sm" data-act="editor-seed-example">From .env.example…</button>
  </div>
  <footer class="modal-foot">
    <span class="modal-note">Kept in your library — nothing is written to disk unless you choose to.</span>
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-accent" data-act="editor-save"${m.busy ? ' disabled' : ''}>${m.busy ? 'Saving…' : 'Save'}</button>
  </footer>`);
}

// --------------------------------------------------------------- handlers

// Row and name inputs store through main's generic `field` input; the
// editor needs no follow-up, so it has no `changed` hook.
export const modals = {
  editor: { view: editorModal },
};

export const actions = {
  'open-editor': run(async () => {
    const v = S.view;
    const rows = await api.editLines(v.id, v.snapshotId);
    openModal({
      kind: 'editor',
      isNew: false,
      projectId: v.id,
      projectName: v.name,
      pathHint: v.pathHint || '',
      rows,
      busy: false,
    });
  }),
  'open-editor-new': () =>
    openModal(
      { kind: 'editor', isNew: true, projectId: null, projectName: '', pathHint: '', rows: [newEntry()], busy: false },
      '#editor-name'
    ),
  'editor-add-entry': () => {
    S.modal.rows.push(newEntry());
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
    const ex = await api.pickExampleFile();
    if (!ex || S.modal !== m) return;
    const existing = new Set(m.rows.filter((r) => r.kind === 'entry').map((r) => r.key));
    const added = ex.exampleKeys
      .filter((k) => !existing.has(k))
      .map((k) => ({ ...newEntry(), key: k }));
    if (!added.length) {
      toast('Those keys are already here');
      return;
    }
    m.rows.push({ kind: 'comment', text: `# from ${ex.exampleName}` }, ...added);
    renderModal();
    toast(`Added ${plural(added.length, 'key', 'keys')} from ${ex.exampleName}`);
  }),
  'editor-save': busy('editor', async (m) => {
    if (m.isNew && !m.projectName.trim()) {
      toast('Give the project a name.', 'error');
      focusNext('#editor-name');
      return;
    }
    const res = await api.saveEditedSnapshot({
      projectId: m.projectId,
      projectName: m.isNew ? m.projectName.trim() : null,
      pathHint: m.isNew ? m.pathHint.trim() || null : null,
      lines: m.rows,
    });
    S.modal = null;
    await loadProjects();
    await selectProject(res.projectId);
    toast(`Saved ${plural(res.entryCount, 'entry', 'entries')}`);
  }),
};

export const inputs = {};
export const forms = {};
