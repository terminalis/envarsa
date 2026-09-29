// Thin wrappers over the IPC surface. Every call goes to the Rust core;
// the UI never touches the filesystem, clipboard, or dialogs itself.
// Values arrive only through an explicit reveal, or seeded into the
// editor by editLines. Paths never go back: picked files and write
// targets come back as opaque tokens, and their paths are for display.
//
// Outside Tauri (plain browser, UI iteration) canned responses stand
// in — see mock.js. It never activates inside the app.
if (!window.__TAURI__) {
  const { installMock } = await import('./mock.js');
  installMock();
}
const { core, event } = window.__TAURI__;
const invoke = (cmd, args) => core.invoke(cmd, args);

export const onEvent = (name, handler) => event.listen(name, handler);

export const api = {
  status: () => invoke('store_status'),
  unlock: (passphrase) => invoke('unlock', { passphrase }),
  lock: () => invoke('lock'),

  listProjects: () => invoke('list_projects'),
  getProject: (projectId, snapshotId = null) => invoke('get_project', { projectId, snapshotId }),

  previewCapture: (text) => invoke('preview_capture', { text }),
  pickEnvFile: () => invoke('pick_env_file'),
  capture: (args) => invoke('capture', { args }),

  revealValue: (projectId, snapshotId, idx) => invoke('reveal_value', { projectId, snapshotId, idx }),
  copyValue: (projectId, snapshotId, idx) => invoke('copy_value', { projectId, snapshotId, idx }),
  copyBlock: (projectId, snapshotId) => invoke('copy_block', { projectId, snapshotId }),
  exportSnapshot: (projectId, snapshotId) => invoke('export_snapshot', { projectId, snapshotId }),

  // Write a project's values out to a .env.local. The target path is
  // staged Rust-side behind an opaque token; only the token comes back.
  // A token from pickExampleFile fills that example instead, and
  // ignores `merge`.
  stageWriteTarget: (projectId, snapshotId) => invoke('stage_write_target', { projectId, snapshotId }),
  pickWriteTarget: (suggestedDir = null) => invoke('pick_write_target', { suggestedDir }),
  pickExampleFile: () => invoke('pick_example_file'),
  previewWrite: (projectId, snapshotId, token, merge) => invoke('preview_write', { projectId, snapshotId, token, merge }),
  writeEnvLocal: (projectId, snapshotId, token, merge) => invoke('write_env_local', { projectId, snapshotId, token, merge }),

  // Structured editor: seed from a snapshot, save back as a new one.
  editLines: (projectId, snapshotId = null) => invoke('edit_lines', { projectId, snapshotId }),
  saveEditedSnapshot: (args) => invoke('save_edited_snapshot', { args }),

  updateProject: (projectId, name, pathHint) => invoke('update_project', { projectId, name, pathHint }),
  deleteProject: (projectId) => invoke('delete_project', { projectId }),
  promoteSnapshot: (projectId, snapshotId) => invoke('promote_snapshot', { projectId, snapshotId }),

  enableEncryption: (passphrase) => invoke('enable_encryption', { passphrase }),
  changePassphrase: (current, newPassphrase) => invoke('change_passphrase', { current, newPassphrase }),
  disableEncryption: (passphrase) => invoke('disable_encryption', { passphrase }),

  revealStore: () => invoke('reveal_store'),
  relocateStore: () => invoke('relocate_store'),
  exportStore: (passphrase = null) => invoke('export_store', { passphrase }),
  // The picker returns an opaque token; inspect/apply present it back.
  // No path ever travels webview → core.
  pickImportStore: () => invoke('pick_import_store'),
  inspectImport: (token, passphrase = null) => invoke('inspect_import', { token, passphrase }),
  applyImport: (token, passphrase, decisions) => invoke('apply_import', { token, passphrase, decisions }),
  restoreBackup: () => invoke('restore_backup'),

  checkForUpdates: () => invoke('check_for_updates'),
  setAutoUpdateCheck: (enabled) => invoke('set_auto_update_check', { enabled }),
  openReleasesPage: () => invoke('open_releases_page'),

  uiLog: (level, message) => invoke('ui_log', { level, message }).catch(() => {}),
};
