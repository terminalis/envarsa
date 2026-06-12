// Pure render functions: state in, HTML out. No IPC calls here.
import { esc, timeAgo, fullTime } from './util.js';

export const ICONS = {
  eye: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7S1 12 1 12z"/><circle cx="12" cy="12" r="3"/></svg>',
  eyeOff: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>',
  copy: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>',
  check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>',
  settings: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/></svg>',
  lock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>',
  unlock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/></svg>',
  trash: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>',
  pencil: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/></svg>',
  x: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>',
  download: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>',
  plus: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>',
  refresh: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>',
  link: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>',
  folder: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
};

export const brandMark = (size = 22) => `
<svg class="brand-mark" width="${size}" height="${size}" viewBox="0 0 24 24" aria-hidden="true">
  <rect x="1.25" y="1.25" width="21.5" height="21.5" rx="5.4" fill="#13151B" stroke="#2C323F" stroke-width="1.1"/>
  <rect x="5.6" y="6.7" width="8.2" height="2.1" rx="1.05" fill="#E5B35A"/>
  <circle cx="6.7" cy="12.05" r="1.05" fill="#7C8599"/>
  <circle cx="9.95" cy="12.05" r="1.05" fill="#7C8599"/>
  <circle cx="13.2" cy="12.05" r="1.05" fill="#7C8599"/>
  <circle cx="16.45" cy="12.05" r="1.05" fill="#7C8599"/>
  <rect x="5.6" y="15.3" width="12.2" height="2.1" rx="1.05" fill="#E5B35A"/>
</svg>`;

const MASK = '<span class="mask" aria-label="hidden value">••••••••••</span>';

// ------------------------------------------------------------------ rail

export function projectListView(S) {
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
      const n = p.entryCount;
      const shared = p.sharedKeys
        ? ` <span class="shared-chip" title="${p.sharedKeys} of these keys also live in other projects">${p.sharedKeys} shared</span>`
        : '';
      return `
<button class="project-item${active}" data-act="select-project" data-id="${esc(p.id)}">
  <span class="project-name">${esc(p.name)}</span>
  <span class="project-meta">${n} ${n === 1 ? 'entry' : 'entries'} · ${esc(timeAgo(p.latestCapturedAt))}${shared}</span>
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
  const sameCount = line.reuse.filter((r) => r.same).length;
  const tone = sameCount > 0 ? ' reuse-same' : '';
  return `<button class="reuse${tone}" data-act="reuse-badge" data-idx="${line.idx}" title="${esc(line.key)} also lives in ${n} other ${n === 1 ? 'project' : 'projects'}"><span class="reuse-ic">${ICONS.link}</span>${n}</button>`;
}

export function rowsView(S) {
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

  return lines
    .map((l) => {
      if (l.t === 'blank') return '<div class="line line-blank"></div>';
      if (l.t === 'comment') return `<div class="line line-comment mono">${esc(l.text)}</div>`;
      if (l.t === 'bad') {
        // A malformed line is as likely as any to hold a secret
        // (a pasted token, a header) — masked like a value.
        const badRevealed = S.revealed.get(l.idx);
        const badCell = badRevealed
          ? `<span class="value-text mono">${esc(badRevealed.value)}</span>`
          : MASK;
        return `
<div class="line line-bad${badRevealed ? ' revealed' : ''}">
  <div class="cell-key">
    <span class="tag tag-bad" title="Kept in the snapshot byte-for-byte, but it isn't a KEY=value line — masked in case it holds a secret">not parsed</span>
  </div>
  <div class="cell-value">${badCell}</div>
  <div class="cell-actions">
    <button class="icon-btn" data-act="${badRevealed ? 'hide' : 'reveal'}" data-idx="${l.idx}" title="${badRevealed ? 'Hide line' : 'Reveal line (auto-hides after 30s)'}">${badRevealed ? ICONS.eyeOff : ICONS.eye}</button>
  </div>
</div>`;
      }

      const revealed = S.revealed.get(l.idx);
      const valueCell = revealed
        ? `<span class="value-text mono">${revealed.value === '' ? '<span class="value-empty">empty</span>' : esc(revealed.value)}</span>`
        : MASK;
      return `
<div class="line line-entry${l.overridden ? ' overridden' : ''}${revealed ? ' revealed' : ''}">
  <div class="cell-key">
    <span class="key mono">${esc(l.key)}</span>
    ${l.exported ? '<span class="tag">export</span>' : ''}
    ${l.overridden ? '<span class="tag" title="A later line in this snapshot overrides this key">overridden</span>' : ''}
    ${reuseBadge(l)}
  </div>
  <div class="cell-value">${valueCell}</div>
  <div class="cell-actions">
    <button class="icon-btn" data-act="${revealed ? 'hide' : 'reveal'}" data-idx="${l.idx}" title="${revealed ? 'Hide value' : 'Reveal value (auto-hides after 30s)'}">${revealed ? ICONS.eyeOff : ICONS.eye}</button>
    <button class="icon-btn" data-act="copy-value" data-idx="${l.idx}" title="Copy value — straight to the clipboard (never shown), cleared after 30s">${ICONS.copy}</button>
  </div>
</div>`;
    })
    .join('');
}

function snapshotOptions(v) {
  return v.snapshots
    .map((s, i) => {
      const label = `${i === 0 ? 'Latest — ' : ''}${fullTime(s.capturedAt)} · ${s.entryCount} ${s.entryCount === 1 ? 'entry' : 'entries'} · ${s.via}`;
      const selected = s.id === v.snapshotId ? ' selected' : '';
      return `<option value="${esc(s.id)}"${selected}>${esc(label)}</option>`;
    })
    .join('');
}

export function hideAllSlotView(S) {
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
  <p class="hero-hint">…or drop a .env file anywhere in this window.</p>
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
    <button class="btn" data-act="export" title="Save this snapshot as a .env file — you choose where"><span class="btn-ic">${ICONS.download}</span>Export</button>
    <button class="icon-btn danger" data-act="open-delete" title="Delete project from the library">${ICONS.trash}</button>
  </div>
  <div class="sub-row">
    ${v.pathHint ? `<span class="path mono" title="Filepath — a note, never a binding; the project's identity is its name">${esc(v.pathHint)}</span><span class="dot">·</span>` : ''}
    <span title="${esc(fullTime(v.capturedAt))}">captured ${esc(timeAgo(v.capturedAt))} via ${esc(v.via)}</span>
    <span class="dot">·</span>
    <span>${v.entryCount} ${v.entryCount === 1 ? 'entry' : 'entries'}</span>
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
      <input type="password" id="unlock-pass" placeholder="Passphrase" autocomplete="current-password">
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

// ---------------------------------------------------------------- modals

export function capturePreviewView(p, busyText) {
  if (busyText) return `<span class="muted">${esc(busyText)}</span>`;
  if (!p) return '<span class="muted">Nothing to capture yet.</span>';
  const bits = [`<strong>${p.entries}</strong> ${p.entries === 1 ? 'entry' : 'entries'}`];
  if (p.comments) bits.push(`${p.comments} ${p.comments === 1 ? 'comment' : 'comments'}`);
  if (p.bad) bits.push(`<span class="warn">${p.bad} ${p.bad === 1 ? 'line' : 'lines'} not parsed</span>`);
  if (p.dupKeys.length) bits.push(`<span class="warn">duplicate: ${esc(p.dupKeys.join(', '))}</span>`);
  return bits.join(' <span class="dot">·</span> ');
}

export function captureProjectHintView(S, m) {
  const name = (m.projectName || '').trim();
  if (!name) return '<span class="muted">Give the snapshot a home.</span>';
  const existing = S.projects.find((p) => p.name.trim().toLowerCase() === name.toLowerCase());
  if (!existing) return `Creates a new project <strong>${esc(name)}</strong>.`;
  const n = existing.entryCount;
  if (!n) return `Becomes the new latest snapshot of <strong>${esc(existing.name)}</strong>, which is empty right now.`;
  const tail =
    m.tab === 'paste'
      ? 'Paste the complete .env, not just what changed; the replaced snapshot stays in history.'
      : 'The project will show exactly what the file contains; the replaced snapshot stays in history.';
  return `<span class="warn">This replaces the current snapshot of <strong>${esc(existing.name)}</strong> — all ${n} ${n === 1 ? 'entry' : 'entries'} — rather than adding to it. ${tail}</span>`;
}

function captureModal(S, m) {
  const datalist = S.projects.map((p) => `<option value="${esc(p.name)}"></option>`).join('');
  const fileTab = `
<div class="file-pick">
  <button class="btn" data-act="pick-file">Choose a .env file…</button>
  ${m.picked
    ? `<span class="file-path mono" title="${esc(m.picked.path)}">${esc(m.picked.path)}</span>`
    : '<span class="muted">or drop one anywhere in the window</span>'}
</div>`;
  const pasteTab = `
<textarea id="capture-text" data-input="capture-text" placeholder="# paste KEY=value lines…" spellcheck="false">${esc(m.pastedText)}</textarea>`;

  return `
<div class="modal modal-capture" role="dialog" aria-label="Capture a snapshot">
  <header class="modal-head">
    <h2>Capture a snapshot</h2>
    <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
  </header>
  <div class="seg">
    <button class="seg-btn${m.tab === 'file' ? ' on' : ''}" data-act="tab-file">From a file</button>
    <button class="seg-btn${m.tab === 'paste' ? ' on' : ''}" data-act="tab-paste">Paste</button>
  </div>
  ${m.tab === 'file' ? fileTab : pasteTab}
  <div class="field">
    <label for="capture-project">Project</label>
    <input id="capture-project" data-input="capture-project" list="project-names" placeholder="Project name" value="${esc(m.projectName)}" autocomplete="off" spellcheck="false">
    <datalist id="project-names">${datalist}</datalist>
    <p class="hint" id="capture-project-hint">${captureProjectHintView(S, m)}</p>
  </div>
  <div class="field">
    <label for="capture-hint">Filepath <span class="muted">(optional)</span></label>
    <input id="capture-hint" data-input="capture-hint" placeholder="C:\\path\\to\\project" value="${esc(m.pathHint)}" autocomplete="off" spellcheck="false">
  </div>
  <div class="preview" id="capture-preview">${capturePreviewView(m.preview)}</div>
  <footer class="modal-foot">
    <span class="modal-note">A capture is the whole file at a point in time — earlier snapshots stay in history.</span>
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-accent" data-act="capture-submit"${m.busy ? ' disabled' : ''}>${m.busy ? 'Capturing…' : 'Capture'}</button>
  </footer>
</div>`;
}

function settingsModal(S, m) {
  const st = S.status;
  const enc = st.encrypted;
  const protection = enc
    ? `
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
</form>`
    : `
<p class="muted">By default the store is plaintext JSON — yours to inspect, back up, and recover with any tool. Masking in the UI keeps values out of casual sight; full-disk encryption is the baseline boundary. Opt in here to also encrypt the file itself.</p>
<form data-form="enc-enable" class="stack">
  <input type="password" name="pass" placeholder="Passphrase (min. 8 characters)" autocomplete="off">
  <input type="password" name="confirm" placeholder="Repeat passphrase" autocomplete="off">
  <label class="check"><input type="checkbox" name="ack"> I understand there is <strong>no recovery</strong> — losing the passphrase means losing the library.</label>
  <p class="form-error"></p>
  <button class="btn btn-accent" type="submit">Encrypt the store</button>
</form>`;

  return `
<div class="modal modal-settings" role="dialog" aria-label="Settings">
  <header class="modal-head">
    <h2>Settings</h2>
    <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
  </header>
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
    </section>
    <section>
      <h3>Protection</h3>
      ${protection}
    </section>
    <section>
      <h3>About</h3>
      <p class="muted">Envarsa ${esc(st.appVersion)} — a local-first library for your environment values. Store-only by design: it copies and exports, but never writes into project trees and never injects into processes. No cloud, no telemetry — the only thing that ever leaves is an update check you trigger or opt into below: one request to GitHub for the latest release number.</p>
      <div class="settings-actions">
        <button class="btn" data-act="check-updates"${m.updChecking ? ' disabled' : ''}>${m.updChecking ? 'Checking…' : 'Check for updates'}</button>
        ${st.updateAvailable ? `<button class="btn btn-accent" data-act="open-releases">Get ${esc(st.updateAvailable)} from GitHub</button>` : ''}
      </div>
      ${m.updError ? `<p class="form-error">${esc(m.updError)}</p>` : ''}
      ${!m.updError && st.updateAvailable ? `<p class="hint">Envarsa <strong>${esc(st.updateAvailable)}</strong> is available — the button opens the releases page in your browser; nothing downloads or installs itself.</p>` : ''}
      ${!m.updError && !st.updateAvailable && m.updDone ? `<p class="hint">You're up to date — ${esc(st.appVersion)} is the latest release.</p>` : ''}
      <label class="check"><input type="checkbox" data-input="auto-update-toggle"${st.autoUpdateCheck ? ' checked' : ''}> Check for updates automatically</label>
      <p class="hint">Off by default. When on, Envarsa asks GitHub for the newest release number shortly after launch, at most once a day — that request is the only network call the app makes, and nothing about your library goes with it.</p>
    </section>
  </div>
</div>`;
}

function exportStoreModal(S, m) {
  const passFields = `
<input type="password" name="pass" id="export-pass" placeholder="Transport passphrase (min. 8 characters)" autocomplete="off">
<input type="password" name="confirm" placeholder="Repeat passphrase" autocomplete="off">
<p class="hint">Standard <span class="mono">age</span> format — importing asks for this passphrase, and <span class="mono">age -d</span> opens the file anywhere, without Envarsa.</p>`;
  const plainNote = S.status?.encrypted
    ? '<p class="hint warn">The copy will be plain, readable JSON — your store’s encryption does not carry over to it.</p>'
    : '<p class="hint">The copy will be plain, readable JSON — same as the store file itself.</p>';
  return `
<div class="modal modal-export-store" role="dialog" aria-label="Export a copy of the store">
  <header class="modal-head">
    <h2>Export a copy</h2>
    <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
  </header>
  <p class="muted">Saves the whole library — every project and its history — as one file, wherever you choose. The store Envarsa keeps using stays where it is.</p>
  <form data-form="export-store" class="stack">
    <label class="check"><input type="checkbox" name="enc" data-input="export-encrypt"${m.encrypt ? ' checked' : ''}> Encrypt the copy for transport</label>
    ${m.encrypt ? passFields : plainNote}
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Choose where to save…</button>
    </footer>
  </form>
</div>`;
}

// What an import would do, given the current decisions: per-row notes
// (HTML, names escaped), how many projects come in, and how many rows
// are still invalid. Pure — used by the renderer and by main.js to
// gate the Import button.
export function importPlan(S, m) {
  const norm = (s) => s.trim().toLowerCase();
  const replaced = new Set();
  m.projects.forEach((p, i) => {
    if (p.conflictsWith && m.decisions[i].action === 'replace') replaced.add(norm(p.conflictsWith));
  });
  const taken = new Set(
    S.projects.map((p) => norm(p.name)).filter((n) => !replaced.has(n))
  );
  let importing = 0;
  let problems = 0;
  const counts = { in: 0, replaces: 0, skips: 0 };
  const rows = new Array(m.projects.length);

  // Fixed names first (add / replace own their incoming name), so a
  // rename that collides with one is flagged on the rename row — the
  // row that has an input to fix.
  m.projects.forEach((p, i) => {
    const d = m.decisions[i];
    if (d.action === 'skip') {
      counts.skips++;
      rows[i] = { ok: true, note: 'Left out — yours stays as it is.' };
      return;
    }
    if (d.action === 'rename') return;
    importing++;
    const final = p.name.trim();
    if (taken.has(norm(final))) {
      problems++;
      rows[i] = { ok: false, note: `“${esc(final)}” is already taken.` };
      return;
    }
    taken.add(norm(final));
    if (d.action === 'replace') {
      counts.replaces++;
      const mine = S.projects.find((x) => norm(x.name) === norm(p.conflictsWith));
      const hist = mine ? ` and its ${mine.snapshotCount} ${mine.snapshotCount === 1 ? 'snapshot' : 'snapshots'}` : '';
      rows[i] = { ok: true, note: `Removes your “${esc(p.conflictsWith)}”${hist}; this one takes its place.` };
    } else {
      counts.in++;
      rows[i] = { ok: true, note: '' };
    }
  });
  m.projects.forEach((p, i) => {
    const d = m.decisions[i];
    if (d.action !== 'rename') return;
    importing++;
    const final = (d.newName || '').trim();
    if (!final) {
      problems++;
      rows[i] = { ok: false, note: 'Give it a name.' };
      return;
    }
    if (taken.has(norm(final))) {
      problems++;
      rows[i] = { ok: false, note: `“${esc(final)}” is already taken.` };
      return;
    }
    taken.add(norm(final));
    counts.in++;
    rows[i] = { ok: true, note: `Comes in as “${esc(final)}”; yours stays untouched.` };
  });
  return { rows, importing, problems, counts };
}

export function importSummaryText(plan) {
  if (plan.importing === 0) return 'Everything is set to skip — nothing to import.';
  const bits = [];
  if (plan.counts.in) bits.push(`adds ${plan.counts.in}`);
  if (plan.counts.replaces) bits.push(`replaces ${plan.counts.replaces}`);
  if (plan.counts.skips) bits.push(`skips ${plan.counts.skips}`);
  return bits.join(' · ');
}

function importRowView(m, p, i, plan) {
  const d = m.decisions[i];
  const when = p.latestCapturedAt ? ` · ${timeAgo(p.latestCapturedAt)}` : '';
  const meta = `${p.snapshotCount} ${p.snapshotCount === 1 ? 'snapshot' : 'snapshots'} · ${p.entryCount} ${p.entryCount === 1 ? 'entry' : 'entries'}${when}`;
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
  return `
<div class="import-row">
  <div class="import-row-main">
    <span class="import-name">${esc(p.name)}</span>
    <span class="import-meta">${esc(meta)} · <span class="warn">name in use</span></span>
  </div>
  <select class="import-action" data-input="import-action" data-idx="${i}" title="What to do about the name conflict">
    <option value="rename"${d.action === 'rename' ? ' selected' : ''}>Import under a new name</option>
    <option value="replace"${d.action === 'replace' ? ' selected' : ''}>Replace yours</option>
    <option value="skip"${d.action === 'skip' ? ' selected' : ''}>Skip</option>
  </select>
  ${d.action === 'rename' ? `<input class="import-rename" id="import-rename-${i}" data-input="import-rename" data-idx="${i}" value="${esc(d.newName)}" placeholder="New name" autocomplete="off" spellcheck="false">` : ''}
  <p class="hint import-row-hint${row.ok ? '' : ' warn'}" id="import-row-hint-${i}">${row.note}</p>
</div>`;
}

function importModal(S, m) {
  const head = `
<header class="modal-head">
  <h2>Import a store</h2>
  <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
</header>
<p class="mono settings-path" title="${esc(m.path)}">${esc(m.path)}</p>`;

  if (!m.unlocked) {
    return `
<div class="modal modal-import" role="dialog" aria-label="Import a store">
  ${head}
  <p class="muted">That store is encrypted. Enter its passphrase — the one it was exported or encrypted with — to see what's inside.</p>
  <form data-form="import-unlock" class="stack">
    <input type="password" name="pass" id="import-pass" placeholder="Passphrase" autocomplete="off">
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Unlock</button>
    </footer>
  </form>
</div>`;
  }

  const plan = importPlan(S, m);
  const body = m.projects.length
    ? `<div class="import-list">${m.projects.map((p, i) => importRowView(m, p, i, plan)).join('')}</div>`
    : '<p class="muted">That store is empty — nothing to import.</p>';
  const disabled = m.busy || plan.problems > 0 || plan.importing === 0;
  return `
<div class="modal modal-import" role="dialog" aria-label="Import a store">
  ${head}
  <p class="muted">Projects merge into your library; the file itself is only read. Conflicting names are yours to settle.</p>
  ${body}
  <footer class="modal-foot">
    <span class="modal-note" id="import-summary">${importSummaryText(plan)}</span>
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-accent" id="import-apply-btn" data-act="import-apply"${disabled ? ' disabled' : ''}>${m.busy ? 'Importing…' : 'Import'}</button>
  </footer>
</div>`;
}

function editModal(S, m) {
  return `
<div class="modal modal-edit" role="dialog" aria-label="Edit project">
  <header class="modal-head">
    <h2>Edit project</h2>
    <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
  </header>
  <form data-form="edit-project" class="stack">
    <div class="field">
      <label for="edit-name">Name <span class="muted">(the project's identity)</span></label>
      <input id="edit-name" name="name" value="${esc(m.name)}" autocomplete="off" spellcheck="false">
    </div>
    <div class="field">
      <label for="edit-hint">Filepath <span class="muted">(optional)</span></label>
      <input id="edit-hint" name="hint" value="${esc(m.pathHint)}" autocomplete="off" spellcheck="false">
    </div>
    <p class="form-error"></p>
    <footer class="modal-foot">
      <button class="btn" type="button" data-act="close-modal">Cancel</button>
      <button class="btn btn-accent" type="submit">Save</button>
    </footer>
  </form>
</div>`;
}

function deleteModal(S, m) {
  return `
<div class="modal modal-delete" role="dialog" aria-label="Delete project">
  <header class="modal-head">
    <h2>Delete “${esc(m.name)}”?</h2>
    <button class="icon-btn" data-act="close-modal" title="Close">${ICONS.x}</button>
  </header>
  <p>Removes the project and its ${m.snapshotCount} ${m.snapshotCount === 1 ? 'snapshot' : 'snapshots'} from the library.</p>
  <p class="muted">The previous version of the store survives as <span class="mono">.bak</span> next to the store file until the next save.</p>
  <footer class="modal-foot">
    <button class="btn" data-act="close-modal">Cancel</button>
    <button class="btn btn-danger" data-act="delete-confirm"${m.busy ? ' disabled' : ''}>${m.busy ? 'Deleting…' : 'Delete project'}</button>
  </footer>
</div>`;
}

export function modalView(S) {
  const m = S.modal;
  if (!m) return '';
  const inner =
    m.kind === 'capture' ? captureModal(S, m)
    : m.kind === 'settings' ? settingsModal(S, m)
    : m.kind === 'export-store' ? exportStoreModal(S, m)
    : m.kind === 'import' ? importModal(S, m)
    : m.kind === 'edit' ? editModal(S, m)
    : m.kind === 'delete' ? deleteModal(S, m)
    : '';
  return `<div class="modal-scrim" data-act="close-modal"></div>${inner}`;
}

// --------------------------------------------------------------- popover

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

// ----------------------------------------------------------------- toasts

export function toastsView(toasts) {
  return toasts
    .map(
      (t) => `
<div class="toast toast-${t.kind}">
  <span class="toast-msg">${esc(t.msg)}</span>
  ${t.detail ? `<span class="toast-detail mono">${esc(t.detail)}</span>` : ''}
</div>`
    )
    .join('');
}
