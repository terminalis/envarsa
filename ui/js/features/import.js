// Import: merge another store's projects into this library, settling
// each name conflict by importing under a new name, replacing yours, or
// skipping it.
import { api } from '../api.js';
import { esc, timeAgo, plural, nameKey, findProject, errText } from '../util.js';
import { modalShell } from '../kit.js';
import {
  S, $, run, busy, toast, openModal, renderModal, focusNext, formError, refreshAfterMutation,
} from '../main.js';

// The name a row brings into the library, or '' when it brings none.
function finalName(p, d) {
  if (d.action === 'skip') return '';
  return d.action === 'rename' ? d.newName.trim() : p.name.trim();
}

// Names a rename suggestion must dodge: every project in the library
// plus the name each other row is set to bring in.
function takenNames(m, exceptIdx) {
  const taken = new Set(S.projects.map((p) => nameKey(p.name)));
  m.projects.forEach((p, j) => {
    const name = j === exceptIdx ? '' : finalName(p, m.decisions[j]);
    if (name) taken.add(nameKey(name));
  });
  return taken;
}

function suggestName(m, i) {
  const base = m.projects[i].name.trim();
  const taken = takenNames(m, i);
  let candidate = `${base} (imported)`;
  for (let n = 2; taken.has(nameKey(candidate)); n++) candidate = `${base} (imported ${n})`;
  return candidate;
}

function openImportModal(preview, passphrase = null) {
  // preview.path is for display only: apply sends the token back.
  const m = {
    kind: 'import',
    ...preview,
    passphrase,
    decisions: preview.projects.map((p) => ({ action: p.conflictsWith ? 'rename' : 'add', newName: '' })),
    busy: false,
  };
  // Each conflict starts as a rename whose suggestion dodges the ones before it.
  m.decisions.forEach((d, i) => {
    if (d.action === 'rename') d.newName = suggestName(m, i);
  });
  openModal(m, preview.unlocked ? null : '#import-pass');
}

// What an import would do, given the current decisions: per-row notes
// (HTML, names escaped), how many projects come in, and how many rows
// are still invalid.
function importPlan(m) {
  const replaced = new Set();
  m.projects.forEach((p, i) => {
    if (p.conflictsWith && m.decisions[i].action === 'replace') replaced.add(nameKey(p.conflictsWith));
  });
  const taken = new Set(S.projects.map((p) => nameKey(p.name)).filter((n) => !replaced.has(n)));
  let importing = 0;
  let problems = 0;
  const counts = { in: 0, replaces: 0, skips: 0 };
  const rows = new Array(m.projects.length);

  // Fixed names first (add and replace own their incoming name), so a
  // rename that collides with one is flagged on the rename row, the row
  // that has an input to fix.
  const isRename = (i) => (m.decisions[i].action === 'rename' ? 1 : 0);
  const order = [...m.projects.keys()].sort((a, b) => isRename(a) - isRename(b));
  for (const i of order) {
    const p = m.projects[i];
    const d = m.decisions[i];
    if (d.action === 'skip') {
      counts.skips++;
      rows[i] = { ok: true, note: 'Left out — yours stays as it is.' };
      continue;
    }
    importing++;
    const name = finalName(p, d);
    if (!name || taken.has(nameKey(name))) {
      problems++;
      rows[i] = { ok: false, note: name ? `“${esc(name)}” is already taken.` : 'Give it a name.' };
      continue;
    }
    taken.add(nameKey(name));
    if (d.action === 'replace') {
      counts.replaces++;
      const mine = findProject(S.projects, p.conflictsWith);
      const hist = mine ? ` and its ${plural(mine.snapshotCount, 'snapshot', 'snapshots')}` : '';
      rows[i] = { ok: true, note: `Removes your “${esc(p.conflictsWith)}”${hist}; this one takes its place.` };
    } else {
      counts.in++;
      rows[i] = { ok: true, note: d.action === 'rename' ? `Comes in as “${esc(name)}”; yours stays untouched.` : '' };
    }
  }
  return { rows, importing, problems, counts };
}

const cannotApply = (m, plan) => m.busy || plan.problems > 0 || plan.importing === 0;

function importSummaryText(plan) {
  if (plan.importing === 0) return 'Everything is set to skip — nothing to import.';
  const bits = [];
  if (plan.counts.in) bits.push(`adds ${plan.counts.in}`);
  if (plan.counts.replaces) bits.push(`replaces ${plan.counts.replaces}`);
  if (plan.counts.skips) bits.push(`skips ${plan.counts.skips}`);
  return bits.join(' · ');
}

// Re-derive the row hints, the summary line and the Import button
// without re-rendering the dialog, so typing a new name keeps its focus.
function updateImportDerived() {
  const m = S.modal;
  const plan = importPlan(m);
  m.projects.forEach((p, i) => {
    const hint = $(`#import-row-hint-${i}`);
    if (hint) {
      hint.innerHTML = plan.rows[i].note;
      hint.classList.toggle('warn', !plan.rows[i].ok);
    }
  });
  $('#import-summary').textContent = importSummaryText(plan);
  $('#import-apply-btn').disabled = cannotApply(m, plan);
}

// ------------------------------------------------------------------ views

function importRowView(m, p, i, plan) {
  const d = m.decisions[i];
  const when = p.latestCapturedAt ? ` · ${timeAgo(p.latestCapturedAt)}` : '';
  const meta = `${plural(p.snapshotCount, 'snapshot', 'snapshots')} · ${plural(p.entryCount, 'entry', 'entries')}${when}`;
  if (!p.conflictsWith) {
    return `
<div class="import-row">
  <div class="import-row-main">
    <span class="import-name">${esc(p.name)}</span>
    <span class="import-meta">${esc(meta)}</span>
  </div>
  <span class="tag tag-new">new</span>
</div>`;
  }
  const row = plan.rows[i];
  const option = (value, label) => `<option value="${value}"${d.action === value ? ' selected' : ''}>${label}</option>`;
  return `
<div class="import-row">
  <div class="import-row-main">
    <span class="import-name">${esc(p.name)}</span>
    <span class="import-meta">${esc(meta)} · <span class="warn">name in use</span></span>
  </div>
  <select class="import-action" data-input="import-action" data-idx="${i}" title="What to do about the name conflict">
    ${option('rename', 'Import under a new name')}
    ${option('replace', 'Replace yours')}
    ${option('skip', 'Skip')}
  </select>
  ${d.action === 'rename' ? `<input class="import-rename" id="import-rename-${i}" data-input="import-rename" data-idx="${i}" value="${esc(d.newName)}" placeholder="New name" autocomplete="off" spellcheck="false">` : ''}
  <p class="hint import-row-hint${row.ok ? '' : ' warn'}" id="import-row-hint-${i}">${row.note}</p>
</div>`;
}

function lockedBody() {
  return `
  <p class="muted">That store is encrypted. Enter its passphrase — the one it was exported or encrypted with — to see what's inside.</p>
  <form data-form="import-unlock" class="stack">
    <input type="password" name="pass" id="import-pass" placeholder="Passphrase" autocomplete="off">
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Unlock</button>
    </footer>
  </form>`;
}

function planBody(m) {
  const plan = importPlan(m);
  const body = m.projects.length
    ? `<div class="import-list">${m.projects.map((p, i) => importRowView(m, p, i, plan)).join('')}</div>`
    : '<p class="muted">That store is empty — nothing to import.</p>';
  return `
  <p class="muted">Projects merge into your library; the file itself is only read. Conflicting names are yours to settle.</p>
  ${body}
  <footer class="modal-foot">
    <span class="modal-note" id="import-summary">${importSummaryText(plan)}</span>
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-accent" id="import-apply-btn" data-act="import-apply"${cannotApply(m, plan) ? ' disabled' : ''}>${m.busy ? 'Importing…' : 'Import'}</button>
  </footer>`;
}

function importModal(S, m) {
  return modalShell({ cls: 'import', label: 'Import a store' }, `
<p class="mono settings-path" title="${esc(m.path)}">${esc(m.path)}</p>
${m.unlocked ? planBody(m) : lockedBody()}`);
}

// --------------------------------------------------------------- handlers

export const modals = {
  import: { view: importModal },
};

export const actions = {
  'open-import-store': run(async () => {
    const token = await api.pickImportStore();
    if (!token) return;
    openImportModal(await api.inspectImport(token));
  }),
  'import-apply': busy('import', async (m) => {
    const plan = importPlan(m);
    if (plan.problems > 0 || plan.importing === 0) return;
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
    toast(`Imported ${plural(total, 'project', 'projects')} into the library`, 'success', bits.join(' · '));
  }),
};

export const inputs = {
  'import-action': (value, el) => {
    const m = S.modal;
    const i = Number(el.dataset.idx);
    const d = m.decisions[i];
    d.action = value;
    if (value === 'rename') {
      if (!d.newName) d.newName = suggestName(m, i);
      focusNext(`#import-rename-${i}`);
    }
    renderModal();
  },
  'import-rename': (value, el) => {
    S.modal.decisions[Number(el.dataset.idx)].newName = value;
    updateImportDerived();
  },
};

export const forms = {
  'import-unlock': async (form) => {
    const m = S.modal;
    const pass = String(new FormData(form).get('pass') || '');
    formError(form, '');
    if (!pass) return formError(form, 'Enter the passphrase.');
    const btn = form.querySelector('button[type="submit"]');
    btn.disabled = true;
    btn.textContent = 'Unlocking…';
    try {
      // Decrypting re-runs scrypt, so a second or two is normal.
      openImportModal(await api.inspectImport(m.token, pass), pass);
    } catch (e) {
      formError(form, errText(e));
      if (btn.isConnected) {
        btn.disabled = false;
        btn.textContent = 'Unlock';
      }
    }
  },
};
