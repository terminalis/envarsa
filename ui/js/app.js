// Shared app state and the helpers every feature uses: rendering,
// dialogs, toasts, handler wrappers and data loading. Features import
// from here; this module imports no feature. The views a render draws
// are handed in by main.js through mountViews once the features load.
import { api } from './api.js';
import { errText } from './util.js';
import { toastsView } from './kit.js';

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

// kind -> { view(S, m), changed?(m, field) }, merged from the features.
// changed runs after `set` or `field` stores a new value, for the
// dialog's own follow-up work.
export const MODALS = {};

// The page's fixed regions, drawn by feature views.
const views = { gate: () => '', rail: () => '', sheet: () => '', popover: () => '' };

// main.js calls this once, after every feature module has loaded.
export function mountViews(regions, modals) {
  Object.assign(views, regions);
  Object.assign(MODALS, modals);
}

let focusAfterRender = null;

// Focus (and select) this element after the next modal render.
export function focusNext(sel) {
  focusAfterRender = sel;
}

export function render() {
  $('#overlay').innerHTML = views.gate(S);
  const unlocked = S.status?.state === 'unlocked';
  $('#app').classList.toggle('gated', !unlocked);
  $('#rail').innerHTML = unlocked ? views.rail(S) : '';
  $('#sheet').innerHTML = unlocked ? views.sheet(S) : '';
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
  $('#popover').innerHTML = views.popover(S);
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

// Forget every revealed value and its auto-hide timer.
export function clearRevealed() {
  for (const r of S.revealed.values()) clearTimeout(r.timer);
  S.revealed.clear();
}

export async function loadProjects() {
  S.projects = await api.listProjects();
  if (S.selId && !S.projects.some((p) => p.id === S.selId)) S.selId = null;
  if (!S.selId && S.projects.length) {
    const saved = localStorage.getItem('envarsa.sel');
    S.selId = S.projects.some((p) => p.id === saved) ? saved : S.projects[0].id;
  }
}

export async function loadView(snapshotId = null) {
  clearRevealed();
  S.filterKeys = '';
  S.view = S.selId ? await api.getProject(S.selId, snapshotId) : null;
}

export async function boot() {
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
