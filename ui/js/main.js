// Registries, wiring and boot. Each feature in features/ owns its views
// and handlers and exports { modals, actions, inputs, forms }; they merge
// here into the tables that data-act, data-input and data-form dispatch
// through. State and the shared helpers live in app.js, which features
// import; nothing imports this module. IPC goes through api.js.
// One-way flow: action -> state -> render.
import { api, onEvent } from './api.js';
import { esc, errText } from './util.js';
import {
  S, $, MODALS, mountViews, renderModal, renderPopover, closeModal, toast, boot,
} from './app.js';
import * as library from './features/library.js';
import * as capture from './features/capture.js';
import * as write from './features/write.js';
import * as editor from './features/editor.js';
import * as importer from './features/import.js';
import * as settings from './features/settings.js';

// ------------------------------------------------------------ registries

const FEATURES = [library, capture, write, editor, importer, settings];
const merge = (part) => Object.assign({}, ...FEATURES.map((f) => f[part]));

mountViews(
  {
    gate: settings.gateView,
    rail: library.railView,
    sheet: library.sheetView,
    popover: library.popoverView,
  },
  merge('modals'),
);

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
  } catch (err) {
    api.uiLog('error', `boot failed: ${errText(err)}`);
    document.body.innerHTML = `<div class="gate"><div class="gate-card"><h1>Envarsa</h1><p class="form-error">Boot failed: ${esc(errText(err))}</p></div></div>`;
  }
})();
