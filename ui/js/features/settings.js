// The store itself: the lock and recovery gate, the settings dialog
// (location, export, encryption, updates) and the export dialog.
import { api } from '../api.js';
import { esc, errText } from '../util.js';
import { ICONS, brandMark, modalShell } from '../kit.js';
import {
  S, $, run, toast, renderModal, openModal, closeModal, focusNext, formError, boot, refreshStatus,
} from '../app.js';

// ------------------------------------------------------------------ gate

export function gateView(S) {
  const st = S.status;
  if (!st) return '<div class="gate"><div class="gate-card"><p class="muted">Starting…</p></div></div>';
  if (st.state === 'locked') {
    return `
<div class="gate">
  <div class="gate-card">
    ${brandMark(56)}
    <h1>Envarsa</h1>
    <p class="muted">This store is encrypted. Enter the passphrase to unlock it.</p>
    <form data-form="unlock" class="gate-form">
      <input type="password" id="unlock-pass" placeholder="Passphrase" autocomplete="current-password" autofocus>
      <button class="btn btn-accent" type="submit">Unlock</button>
    </form>
    <p class="form-error" id="unlock-error"></p>
    <p class="gate-path mono" title="${esc(st.storePath)}">${esc(st.storePath)}</p>
  </div>
</div>`;
  }
  if (st.state === 'corrupt') {
    return `
<div class="gate">
  <div class="gate-card gate-wide">
    ${brandMark(48)}
    <h1>The store could not be loaded</h1>
    <pre class="error-block mono">${esc(st.error || 'unknown error')}</pre>
    <p class="muted">The store file is the source of truth and Envarsa won't touch it while it can't read it. A one-step backup (<span class="mono">.bak</span>) sits next to it after every save.</p>
    <div class="gate-actions">
      <button class="btn" data-act="reveal-store"><span class="btn-ic">${ICONS.folder}</span>Show the file</button>
      ${st.backupExists ? '<button class="btn" data-act="restore-backup">Restore the backup</button>' : ''}
      <button class="btn btn-ghost" data-act="retry-boot">Try again</button>
    </div>
    <p class="gate-path mono">${esc(st.storePath)}</p>
  </div>
</div>`;
  }
  return '';
}

// ------------------------------------------------------------ settings

// The About paragraph; packaged and direct builds differ only in how it ends.
const aboutText = (st, ending) =>
  `<p class="muted">Envarsa ${esc(st.appVersion)} — a local-first library for your environment values. It copies and exports, and never injects into processes; the one way it writes into a project tree is an explicit, guarded export to a <span class="mono">.env*.local</span> (never a committed example file). No cloud, no telemetry${ending}</p>`;

function aboutSection(st, m) {
  if (st.packaged) {
    return `
      ${aboutText(st, ', and no network calls.')}
      <p class="hint">Installed from the Microsoft Store — updates arrive through the Store automatically, so the in-app update check is off.</p>`;
  }
  return `
      ${aboutText(st, ' — the only thing that ever leaves is an update check you trigger or opt into below: one request to GitHub for the latest release number.')}
      <div class="settings-actions">
        <button class="btn" data-act="check-updates"${m.updChecking ? ' disabled' : ''}>${m.updChecking ? 'Checking…' : 'Check for updates'}</button>
        ${st.updateAvailable ? `<button class="btn btn-accent" data-act="open-releases">Get ${esc(st.updateAvailable)} from GitHub</button>` : ''}
      </div>
      ${m.updError ? `<p class="form-error">${esc(m.updError)}</p>` : ''}
      ${!m.updError && st.updateAvailable ? `<p class="hint">Envarsa <strong>${esc(st.updateAvailable)}</strong> is available — the button opens the releases page in your browser; nothing downloads or installs itself.</p>` : ''}
      ${!m.updError && !st.updateAvailable && m.updDone ? `<p class="hint">You're up to date — ${esc(st.appVersion)} is the latest release.</p>` : ''}
      <label class="check"><input type="checkbox" data-input="auto-update-toggle"${st.autoUpdateCheck ? ' checked' : ''}> Check for updates automatically</label>
      <p class="hint">Off by default. When on, Envarsa asks GitHub for the newest release number shortly after launch, at most once a day — that request is the only network call the app makes, and nothing about your library goes with it.</p>`;
}

function protectionSection(st) {
  if (st.encrypted) {
    return `
<p class="muted">The store is encrypted at rest with a passphrase (standard <span class="mono">age</span> format, scrypt). You can always decrypt the file yourself: <span class="mono">age -d envarsa.store</span></p>
<div class="settings-actions"><button class="btn" data-act="lock"><span class="btn-ic">${ICONS.lock}</span>Lock now</button></div>
<form data-form="enc-change" class="stack">
  <h4>Change passphrase</h4>
  <input type="password" name="current" placeholder="Current passphrase" autocomplete="off">
  <input type="password" name="next" placeholder="New passphrase (min. 8 characters)" autocomplete="off">
  <input type="password" name="confirm" placeholder="Repeat new passphrase" autocomplete="off">
  <p class="form-error"></p>
  <button class="btn" type="submit">Change passphrase</button>
</form>
<form data-form="enc-disable" class="stack">
  <h4>Turn encryption off</h4>
  <p class="muted">The store goes back to plaintext JSON on disk.</p>
  <input type="password" name="current" placeholder="Current passphrase" autocomplete="off">
  <p class="form-error"></p>
  <button class="btn btn-danger-ghost" type="submit">Decrypt the store</button>
</form>`;
  }
  return `
<p class="muted">By default the store is plaintext JSON — yours to inspect, back up, and recover with any tool. Masking in the UI keeps values out of casual sight; full-disk encryption is the baseline boundary. Opt in here to also encrypt the file itself.</p>
<form data-form="enc-enable" class="stack">
  <input type="password" name="pass" placeholder="Passphrase (min. 8 characters)" autocomplete="off">
  <input type="password" name="confirm" placeholder="Repeat passphrase" autocomplete="off">
  <label class="check"><input type="checkbox" name="ack"> I understand there is <strong>no recovery</strong> — losing the passphrase means losing the library.</label>
  <p class="form-error"></p>
  <button class="btn btn-accent" type="submit">Encrypt the store</button>
</form>`;
}

function settingsModal(S, m) {
  const st = S.status;
  return modalShell({ cls: 'settings', label: 'Settings' }, `
  <div class="modal-scroll">
    <section>
      <h3>Store file</h3>
      <p class="mono settings-path" title="${esc(st.storePath)}">${esc(st.storePath)}</p>
      ${st.envOverride ? '<p class="hint warn">Location forced by <span class="mono">ENVARSA_STORE_PATH</span> for this run.</p>' : ''}
      <p class="muted">One portable file holds everything${st.backupExists ? ' — a one-step <span class="mono">.bak</span> sits next to it' : ''}. Portability is manual and yours: export a copy to carry over (optionally encrypted for the trip), import another store's projects into this one, or keep the file in a folder you sync yourself.</p>
      <div class="settings-actions">
        <button class="btn" data-act="reveal-store"><span class="btn-ic">${ICONS.folder}</span>Show in Explorer</button>
        <button class="btn" data-act="relocate-store" title="Pick a new home for the store file — Envarsa moves it there and keeps using it from then on">Change location…</button>
        <button class="btn" data-act="open-export-store"><span class="btn-ic">${ICONS.download}</span>Export…</button>
        <button class="btn" data-act="open-import-store">Import…</button>
      </div>
      ${st.portable ? '<p class="hint">Portable build: the store and your settings live in this folder, so they travel with it. <strong>Change location&hellip;</strong> can move the store elsewhere, but a spot outside this folder will not travel when you move the folder.</p>' : ''}
    </section>
    <section>
      <h3>Protection</h3>
      ${protectionSection(st)}
    </section>
    <section>
      <h3>About</h3>
      ${aboutSection(st, m)}
    </section>
  </div>`);
}

function exportStoreModal(S, m) {
  const passFields = `
<input type="password" name="pass" id="export-pass" placeholder="Transport passphrase (min. 8 characters)" autocomplete="off">
<input type="password" name="confirm" placeholder="Repeat passphrase" autocomplete="off">
<p class="hint">Standard <span class="mono">age</span> format — importing asks for this passphrase, and <span class="mono">age -d</span> opens the file anywhere, without Envarsa.</p>`;
  const plainNote = S.status.encrypted
    ? '<p class="hint warn">The copy will be plain, readable JSON — your store’s encryption does not carry over to it.</p>'
    : '<p class="hint">The copy will be plain, readable JSON — same as the store file itself.</p>';
  return modalShell({ cls: 'export-store', label: 'Export a copy of the store', title: 'Export a copy' }, `
  <p class="muted">Saves the whole library — every project and its history — as one file, wherever you choose. The store Envarsa keeps using stays where it is.</p>
  <form data-form="export-store" class="stack">
    <label class="check"><input type="checkbox" name="enc" data-input="field" data-field="encrypt"${m.encrypt ? ' checked' : ''}> Encrypt the copy for transport</label>
    ${m.encrypt ? passFields : plainNote}
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Choose where to save…</button>
    </footer>
  </form>`);
}

// ---------------------------------------------------------------- updates

// classList only, no innerHTML: an auto-check landing seconds after boot
// can never steal typing focus from the rail filter.
function applyUpdateBadge() {
  document.querySelectorAll('[data-act="open-settings"].icon-btn').forEach((b) =>
    b.classList.toggle('has-update', !!S.status?.updateAvailable));
}

// The background auto-check (opt-in) found a newer release.
export function updateAvailable(version) {
  if (S.status) S.status.updateAvailable = version;
  applyUpdateBadge();
  if (S.modal?.kind === 'settings') renderModal();
}

// --------------------------------------------------------------- handlers

export const modals = {
  settings: { view: settingsModal },
  'export-store': {
    view: exportStoreModal,
    // `field` stores the encrypt checkbox; the form then grows or drops
    // its passphrase inputs.
    changed: (m) => {
      if (m.encrypt) focusNext('#export-pass');
      renderModal();
    },
  },
};

export const actions = {
  lock: run(async () => {
    await api.lock();
    await boot();
  }),
  'open-settings': run(async () => {
    await refreshStatus();
    openModal({ kind: 'settings', updChecking: false, updDone: false, updError: null });
  }),
  // Not run()-wrapped: a failed manual check reports inline in the
  // settings dialog, not as a toast.
  'check-updates': async () => {
    const m = S.modal;
    Object.assign(m, { updChecking: true, updDone: false, updError: null });
    renderModal();
    try {
      const res = await api.checkForUpdates();
      S.status.updateAvailable = res.updateAvailable ? res.latest : null;
      m.updDone = true;
    } catch (e) {
      m.updError = errText(e);
    } finally {
      m.updChecking = false;
      if (S.modal === m) renderModal();
      applyUpdateBadge();
    }
  },
  'open-releases': run(async () => api.openReleasesPage()),
  'reveal-store': run(async () => api.revealStore()),
  'relocate-store': run(async () => {
    const path = await api.relocateStore();
    if (!path) return;
    await refreshStatus(); // the settings dialog re-renders with the new path
    toast('Store moved', 'success', path);
  }),
  'open-export-store': () =>
    // Default to encrypting the copy when the store itself is encrypted.
    openModal({ kind: 'export-store', encrypt: S.status.encrypted }, S.status.encrypted ? '#export-pass' : null),
  'restore-backup': run(async () => {
    await api.restoreBackup();
    await boot();
    toast('Backup restored');
  }),
  'retry-boot': run(async () => boot()),
};

export const inputs = {
  'auto-update-toggle': async (value, el) => {
    try {
      await api.setAutoUpdateCheck(el.checked);
      S.status.autoUpdateCheck = el.checked;
    } catch (e) {
      el.checked = !el.checked;
      toast(errText(e), 'error');
    }
  },
};

// Passphrase rules checked before the round trip; Rust checks them too.
function passphraseProblem(pass, confirm, mismatch = 'The passphrases do not match.') {
  if (String(pass).length < 8) return 'Use at least 8 characters.';
  if (pass !== confirm) return mismatch;
  return '';
}

export const forms = {
  unlock: async () => {
    const input = $('#unlock-pass');
    const error = $('#unlock-error');
    error.textContent = '';
    try {
      await api.unlock(input.value);
      await boot();
    } catch (e) {
      error.textContent = errText(e);
      input.select();
    }
  },
  'enc-enable': async (form) => {
    const f = new FormData(form);
    const problem =
      passphraseProblem(f.get('pass'), f.get('confirm')) ||
      (f.get('ack') ? '' : 'Tick the box — this is the one promise Envarsa cannot break for you.');
    formError(form, problem);
    if (problem) return;
    try {
      await api.enableEncryption(String(f.get('pass')));
      await refreshStatus();
      toast('Store encrypted — keep that passphrase safe');
    } catch (e) {
      formError(form, errText(e));
    }
  },
  'enc-change': async (form) => {
    const f = new FormData(form);
    const problem = passphraseProblem(f.get('next'), f.get('confirm'), 'The new passphrases do not match.');
    formError(form, problem);
    if (problem) return;
    try {
      await api.changePassphrase(String(f.get('current')), String(f.get('next')));
      renderModal(); // clears the form
      toast('Passphrase changed');
    } catch (e) {
      formError(form, errText(e));
    }
  },
  'enc-disable': async (form) => {
    formError(form, '');
    try {
      await api.disableEncryption(String(new FormData(form).get('current')));
      await refreshStatus();
      toast('Store decrypted — plaintext JSON on disk again');
    } catch (e) {
      formError(form, errText(e));
    }
  },
  'export-store': async (form) => {
    const m = S.modal;
    const f = new FormData(form);
    let passphrase = null;
    if (m.encrypt) {
      passphrase = String(f.get('pass') || '');
      const problem = passphraseProblem(passphrase, f.get('confirm'));
      formError(form, problem);
      if (problem) return;
    } else {
      formError(form, '');
    }
    // No re-render around the await: the typed passphrase stays put if
    // the user cancels the save dialog or the write fails.
    const btn = form.querySelector('button[type="submit"]');
    btn.disabled = true;
    btn.textContent = 'Exporting…';
    try {
      const path = await api.exportStore(passphrase);
      if (path) {
        closeModal();
        toast('Exported a copy of the store', 'success', path);
      }
    } catch (e) {
      formError(form, errText(e));
    } finally {
      if (btn.isConnected) {
        btn.disabled = false;
        btn.textContent = 'Choose where to save…';
      }
    }
  },
};
