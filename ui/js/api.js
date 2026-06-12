// Thin wrappers over the IPC surface. Every call goes to the Rust core;
// the UI never touches the filesystem, clipboard, or dialogs itself.
// Values only ever arrive here through an explicit reveal.
//
// Outside Tauri (plain browser, UI iteration) a fixture-backed mock
// stands in — see mock.js. It never activates inside the app.
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

  renameProject: (projectId, name) => invoke('rename_project', { projectId, name }),
  setPathHint: (projectId, pathHint) => invoke('set_path_hint', { projectId, pathHint }),
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

  selftest: {
    enabled: () => invoke('selftest_enabled'),
    readClipboard: () => invoke('selftest_read_clipboard'),
    setClipboard: (text) => invoke('selftest_set_clipboard', { text }),
    readFile: (path) => invoke('selftest_read_file', { path }),
    stageImport: (path) => invoke('selftest_stage_import', { path }),
    exportToPath: (projectId, snapshotId, path) => invoke('export_to_path', { projectId, snapshotId, path }),
    exportStoreToPath: (path, passphrase = null) => invoke('export_store_to_path', { path, passphrase }),
    done: (passed, failed, report) => invoke('selftest_done', { passed, failed, report }),
  },
};
