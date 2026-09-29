// State, render, registries, wiring and boot. Each feature in features/
// owns its views and handlers and exports { modals, actions, inputs,
// forms }; they merge here into the tables that data-act, data-input
// and data-form dispatch through. IPC goes through api.js.
// One-way flow: action -> state -> render.
//
// Features import their helpers from this module, which imports them
// back. That cycle is safe: features call these helpers only once events
// arrive, except run() and busy(), which wrap handlers while the feature
// modules load. Both are function declarations, so they exist before
// this module's body runs.
import { api, onEvent } from './api.js';
import { esc, errText } from './util.js';
import { toastsView } from './kit.js';
import { runSelftest } from './selftest.js';
import * as library from './features/library.js';
import * as capture from './features/capture.js';
import * as write from './features/write.js';
import * as editor from './features/editor.js';
import * as importer from './features/import.js';
import * as settings from './features/settings.js';

export const S = {
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

export const $ = (sel) => document.querySelector(sel);

// ------------------------------------------------------------- rendering

let focusAfterRender = null;

// Focus (and select) this element after the next modal render.
export function focusNext(sel) {
  focusAfterRender = sel;
}

export function render() {
  $('#overlay').innerHTML = settings.gateView(S);
  const unlocked = S.status?.state === 'unlocked';
  $('#app').classList.toggle('gated', !unlocked);
  $('#rail').innerHTML = unlocked ? library.railView(S) : '';
  $('#sheet').innerHTML = unlocked ? library.sheetView(S) : '';
  renderModal();
  renderPopover();
  $('#overlay [autofocus]')?.focus();
}

export function renderModal() {
  const modal = S.modal && MODALS[S.modal.kind];
  $('#modal').innerHTML = modal
    ? `<div class="modal-scrim" data-act="close-modal"></div>${modal.view(S, S.modal)}`
    : '';
  if (focusAfterRender) {
    const el = $(focusAfterRender);
    el?.focus();
    el?.select?.();
    focusAfterRender = null;
  }
}

export function renderPopover() {
  $('#popover').innerHTML = library.popoverView(S);
}

export function openModal(m, focusSel = null) {
  S.modal = m;
  focusAfterRender = focusSel;
  renderModal();
}

export function closeModal() {
  S.modal = null;
  renderModal();
}

function renderToasts() {
  $('#toasts').innerHTML = toastsView(S.toasts);
}

export function toast(msg, kind = 'success', detail = null) {
  const t = { msg, kind, detail };
  S.toasts.push(t);
  renderToasts();
  setTimeout(() => {
    S.toasts = S.toasts.filter((x) => x !== t);
    renderToasts();
  }, kind === 'error' ? 6000 : 3200);
}

export function formError(form, msg) {
  const el = form.querySelector('.form-error');
  if (el) el.textContent = msg || '';
}

// A handler whose failure becomes an error toast.
export function run(fn) {
  return async (...args) => {
    try {
      await fn(...args);
    } catch (err) {
      toast(errText(err), 'error');
    }
  };
}

// A dialog action that locks its dialog while it runs. A busy dialog
// refuses Close, Escape and the scrim, so the lock always comes off.
export function busy(kind, fn) {
  return run(async (...args) => {
    const m = S.modal;
    if (m?.kind !== kind || m.busy) return;
    m.busy = true;
    renderModal();
    try {
      await fn(m, ...args);
    } finally {
      if (S.modal === m) {
        m.busy = false;
        renderModal();
      }
    }
  });
}

// ------------------------------------------------------------- data flow

export async function loadProjects() {
  S.projects = await api.listProjects();
  if (S.selId && !S.projects.some((p) => p.id === S.selId)) S.selId = null;
  if (!S.selId && S.projects.length) {
    const saved = localStorage.getItem('envarsa.sel');
    S.selId = S.projects.some((p) => p.id === saved) ? saved : S.projects[0].id;
  }
}

export async function loadView(snapshotId = null) {
  library.clearRevealed();
  S.filterKeys = '';
  S.view = S.selId ? await api.getProject(S.selId, snapshotId) : null;
}

export async function boot() {
  S.status = await api.status();
  S.modal = null;
  S.popover = null;
  library.clearRevealed();
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

// Reload the list and the selected project's latest snapshot.
export async function refreshAfterMutation() {
  await loadProjects();
  await loadView();
  render();
}

export async function refreshStatus() {
  S.status = await api.status();
  render();
}

export async function selectProject(id) {
  S.selId = id;
  localStorage.setItem('envarsa.sel', id);
  await loadView();
  render();
}

// ------------------------------------------------------------ registries

const FEATURES = [library, capture, write, editor, importer, settings];
const merge = (part) => Object.assign({}, ...FEATURES.map((f) => f[part]));

// kind -> { view(S, m), changed?(m, field) }. changed runs after `set`
// or `field` stores a new value, for the dialog's own follow-up work.
const MODALS = merge('modals');

const ACTIONS = {
  ...merge('actions'),
  // Segmented buttons: data-field names a field of the open dialog,
  // data-value the value to give it.
  set: (d) => {
    const m = S.modal;
    m[d.field] = d.value;
    MODALS[m.kind].changed?.(m, d.field);
    renderModal();
  },
  'close-modal': () => {
    if (!S.modal?.busy) closeModal();
  },
};

const INPUTS = {
  ...merge('inputs'),
  // Live dialog inputs store as the user types, without a re-render, so
  // focus survives. data-field names the field; data-idx, when present,
  // picks one of the dialog's rows instead.
  field: (value, el) => {
    const m = S.modal;
    const { field, idx } = el.dataset;
    const target = idx === undefined ? m : m.rows[Number(idx)];
    target[field] = el.type === 'checkbox' ? el.checked : value;
    MODALS[m.kind].changed?.(m, field);
  },
};

const FORMS = merge('forms');

// ----------------------------------------------------------------- wiring

document.addEventListener('click', (e) => {
  if (S.popover && !e.target.closest('.popover') && !e.target.closest('[data-act="reuse-badge"]')) {
    S.popover = null;
    renderPopover();
  }
  const el = e.target.closest('[data-act]');
  if (!el) return;
  ACTIONS[el.dataset.act]?.(el.dataset, el, e);
});

document.addEventListener('input', (e) => {
  const key = e.target?.dataset?.input;
  if (key) INPUTS[key]?.(e.target.value, e.target);
});

document.addEventListener('submit', (e) => {
  const kind = e.target?.dataset?.form;
  if (!kind) return;
  e.preventDefault();
  FORMS[kind]?.(e.target);
});

document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  if (S.popover) {
    S.popover = null;
    renderPopover();
  } else if (S.modal && !S.modal.busy) {
    closeModal();
  }
});

// Privacy reflex: anything revealed hides when the window loses focus.
window.addEventListener('blur', library.hideRevealed);

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
  if (e?.payload) capture.applyPickedFile(e.payload);
});
onEvent('env-drop-error', (e) => {
  toast(String(e?.payload || 'could not read the dropped file'), 'error');
});
onEvent('update-available', (e) => {
  if (typeof e?.payload === 'string' && e.payload) settings.updateAvailable(e.payload);
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
    api.uiLog('error', `boot failed: ${errText(err)}`);
    document.body.innerHTML = `<div class="gate"><div class="gate-card"><h1>Envarsa</h1><p class="form-error">Boot failed: ${esc(errText(err))}</p></div></div>`;
  }
})();
