//! The IPC surface. The webview can only reach these commands; every
//! OS interaction (file dialogs, clipboard, shell reveal) stays in the
//! Rust core. Values cross into the webview only on an explicit
//! per-value reveal — listing a project sends keys and structure, never
//! values. "Copy" hands a value straight from the core to the OS
//! clipboard without it ever transiting the UI.

use crate::envfile::{self, Line};
use crate::state::{self, AppState, Inner, Session};
use crate::store::{self, Project, Snapshot, Store};
use crate::crypto;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

fn selftest_active() -> bool {
    std::env::var("ENVARSA_SELFTEST").is_ok()
}

fn with_inner<T>(state: &State<'_, AppState>, f: impl FnOnce(&mut Inner) -> R<T>) -> R<T> {
    let mut guard = state.0.lock().map_err(|_| "internal: state poisoned".to_string())?;
    let inner = guard.as_mut().ok_or_else(|| "app is still starting".to_string())?;
    f(inner)
}

/// Read-only access to the unlocked store.
fn with_store<T>(state: &State<'_, AppState>, f: impl FnOnce(&Store) -> R<T>) -> R<T> {
    with_inner(state, |inner| match &inner.session {
        Session::Unlocked { store, .. } => f(store),
        Session::Locked => Err("the store is locked".into()),
        Session::Corrupt { error } => Err(format!("the store could not be loaded: {error}")),
    })
}

/// Mutate the unlocked store, then persist it durably. The mutation is
/// only kept if the save succeeds.
fn mutate<T>(state: &State<'_, AppState>, f: impl FnOnce(&mut Store) -> R<T>) -> R<T> {
    with_inner(state, |inner| {
        let path = inner.store_path.clone();
        match &mut inner.session {
            Session::Unlocked { store, passphrase } => {
                let before = store.clone();
                match f(store) {
                    Ok(out) => match store::save(store, &path, passphrase.as_deref()) {
                        Ok(()) => Ok(out),
                        Err(e) => {
                            *store = before; // roll back the in-memory state
                            Err(format!("could not save the store: {e}"))
                        }
                    },
                    Err(e) => {
                        *store = before;
                        Err(e)
                    }
                }
            }
            Session::Locked => Err("the store is locked".into()),
            Session::Corrupt { error } => {
                Err(format!("the store could not be loaded: {error}"))
            }
        }
    })
}

// ---------------------------------------------------------------- status

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub state: String, // "unlocked" | "locked" | "corrupt"
    pub store_path: String,
    pub encrypted: bool,
    pub env_override: bool,
    pub backup_exists: bool,
    pub project_count: Option<usize>,
    pub error: Option<String>,
    pub app_version: String,
    /// A newer released version, when the last (manual or automatic)
    /// check found one. Re-validated here so a stale or garbage
    /// persisted value can never badge the UI.
    pub update_available: Option<String>,
    pub auto_update_check: bool,
}

fn status_of(app: &AppHandle, inner: &Inner) -> StatusPayload {
    let (state_str, encrypted, project_count, error) = match &inner.session {
        Session::Unlocked { store, passphrase } => (
            "unlocked",
            passphrase.is_some(),
            Some(store.projects.len()),
            None,
        ),
        Session::Locked => ("locked", true, None, None),
        Session::Corrupt { error } => ("corrupt", false, None, Some(error.clone())),
    };
    StatusPayload {
        state: state_str.to_string(),
        store_path: inner.store_path.to_string_lossy().to_string(),
        encrypted,
        env_override: inner.env_override,
        backup_exists: store::backup_path(&inner.store_path).exists(),
        project_count,
        error,
        app_version: app.package_info().version.to_string(),
        update_available: inner
            .config
            .available_version
            .as_deref()
            .and_then(|t| crate::update::parse_tag(t).ok())
            .filter(|v| *v > app.package_info().version)
            .map(|v| v.to_string()),
        auto_update_check: inner.config.auto_update_check,
    }
}

#[tauri::command]
pub fn store_status(app: AppHandle, state: State<'_, AppState>) -> R<StatusPayload> {
    with_inner(&state, |inner| Ok(status_of(&app, inner)))
}

#[tauri::command]
pub fn unlock(state: State<'_, AppState>, passphrase: String) -> R<()> {
    with_inner(&state, |inner| {
        let bytes = fs::read(&inner.store_path)
            .map_err(|e| format!("could not read store file: {e}"))?;
        if crypto::is_encrypted(&bytes) {
            let plain = crypto::decrypt(&bytes, &passphrase)?;
            let s = store::parse_store(&plain)?;
            inner.session = Session::Unlocked {
                store: s,
                passphrase: Some(passphrase),
            };
        } else {
            // The file is plaintext after all (e.g. encryption was
            // disabled elsewhere) — just load it.
            let s = store::parse_store(&bytes)?;
            inner.session = Session::Unlocked {
                store: s,
                passphrase: None,
            };
        }
        Ok(())
    })
}

#[tauri::command]
pub fn lock(state: State<'_, AppState>) -> R<()> {
    with_inner(&state, |inner| match &inner.session {
        Session::Unlocked {
            passphrase: Some(_),
            ..
        } => {
            inner.session = Session::Locked;
            Ok(())
        }
        Session::Unlocked { .. } => Err("the store is not encrypted — nothing to lock".into()),
        _ => Ok(()),
    })
}

// -------------------------------------------------------------- projects

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub path_hint: Option<String>,
    pub snapshot_count: usize,
    pub entry_count: usize,
    pub latest_captured_at: Option<String>,
    /// How many of this project's keys also exist in other projects.
    pub shared_keys: usize,
}

fn latest_effective(project: &Project) -> Vec<(String, String)> {
    project
        .latest()
        .map(|s| envfile::effective_entries(&envfile::parse(&s.raw)))
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> R<Vec<ProjectMeta>> {
    with_store(&state, |store| {
        let all_effective: Vec<(String, Vec<(String, String)>)> = store
            .projects
            .iter()
            .map(|p| (p.id.clone(), latest_effective(p)))
            .collect();

        let mut metas: Vec<ProjectMeta> = store
            .projects
            .iter()
            .map(|p| {
                let mine = latest_effective(p);
                let shared = mine
                    .iter()
                    .filter(|(k, _)| {
                        all_effective.iter().any(|(other_id, entries)| {
                            other_id != &p.id && entries.iter().any(|(ok, _)| ok == k)
                        })
                    })
                    .count();
                ProjectMeta {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    path_hint: p.path_hint.clone(),
                    snapshot_count: p.snapshots.len(),
                    entry_count: mine.len(),
                    latest_captured_at: p.latest().map(|s| s.captured_at.clone()),
                    shared_keys: shared,
                }
            })
            .collect();
        metas.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(metas)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReuseRef {
    pub project_id: String,
    pub name: String,
    pub same: bool,
}

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum LineView {
    Blank,
    Comment {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Entry {
        idx: usize,
        key: String,
        exported: bool,
        /// True when a later line in the same snapshot overrides this key.
        overridden: bool,
        reuse: Vec<ReuseRef>,
    },
    /// The raw text stays out of the listing — a malformed line is as
    /// likely as any to hold a secret, so it is masked like a value and
    /// crosses only through `reveal_value`.
    Bad {
        idx: usize,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub captured_at: String,
    pub via: String,
    pub source_path: Option<String>,
    pub entry_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub path_hint: Option<String>,
    pub created_at: String,
    /// Newest first.
    pub snapshots: Vec<SnapshotMeta>,
    pub snapshot_id: String,
    pub captured_at: String,
    pub via: String,
    pub source_path: Option<String>,
    pub is_latest: bool,
    pub entry_count: usize,
    pub lines: Vec<LineView>,
}

#[tauri::command]
pub fn get_project(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: Option<String>,
) -> R<ProjectView> {
    with_store(&state, |store| {
        let project = store
            .project(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let snapshot = match &snapshot_id {
            Some(id) => project
                .snapshot(id)
                .ok_or_else(|| "snapshot not found".to_string())?,
            None => project
                .latest()
                .ok_or_else(|| "project has no snapshots".to_string())?,
        };
        let is_latest = project.latest().map(|s| s.id == snapshot.id).unwrap_or(false);

        // Other projects' current entries, for reuse flags.
        let others: Vec<(&Project, Vec<(String, String)>)> = store
            .projects
            .iter()
            .filter(|p| p.id != project.id)
            .map(|p| (p, latest_effective(p)))
            .collect();

        let lines = envfile::parse(&snapshot.raw);

        // Which keys are overridden by a later line in this snapshot?
        let mut last_idx: std::collections::HashMap<&str, usize> = Default::default();
        for (i, line) in lines.iter().enumerate() {
            if let Line::Entry { key, .. } = line {
                last_idx.insert(key.as_str(), i);
            }
        }

        let line_views = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| match line {
                Line::Blank => LineView::Blank,
                Line::Comment(text) => LineView::Comment { text: text.clone() },
                Line::Bad(_) => LineView::Bad { idx },
                Line::Entry {
                    key,
                    value,
                    exported,
                } => {
                    let reuse = others
                        .iter()
                        .filter_map(|(p, entries)| {
                            entries.iter().find(|(k, _)| k == key).map(|(_, v)| ReuseRef {
                                project_id: p.id.clone(),
                                name: p.name.clone(),
                                same: v == value,
                            })
                        })
                        .collect();
                    LineView::Entry {
                        idx,
                        key: key.clone(),
                        exported: *exported,
                        overridden: last_idx.get(key.as_str()) != Some(&idx),
                        reuse,
                    }
                }
            })
            .collect();

        Ok(ProjectView {
            id: project.id.clone(),
            name: project.name.clone(),
            path_hint: project.path_hint.clone(),
            created_at: project.created_at.clone(),
            snapshots: project
                .snapshots
                .iter()
                .rev()
                .map(|s| SnapshotMeta {
                    id: s.id.clone(),
                    captured_at: s.captured_at.clone(),
                    via: s.via.clone(),
                    source_path: s.source_path.clone(),
                    entry_count: envfile::entry_count(&s.raw),
                })
                .collect(),
            snapshot_id: snapshot.id.clone(),
            captured_at: snapshot.captured_at.clone(),
            via: snapshot.via.clone(),
            source_path: snapshot.source_path.clone(),
            is_latest,
            entry_count: envfile::entry_count(&snapshot.raw),
            lines: line_views,
        })
    })
}

// --------------------------------------------------------------- capture

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePreview {
    pub entries: usize,
    pub comments: usize,
    pub bad: usize,
    pub keys: Vec<String>,
    pub dup_keys: Vec<String>,
}

#[tauri::command]
pub fn preview_capture(text: String) -> R<CapturePreview> {
    let lines = envfile::parse(&text);
    let mut keys = Vec::new();
    let mut dups = Vec::new();
    let mut comments = 0;
    let mut bad = 0;
    for line in &lines {
        match line {
            Line::Comment(_) => comments += 1,
            Line::Bad(_) => bad += 1,
            Line::Entry { key, .. } => {
                if keys.contains(key) {
                    if !dups.contains(key) {
                        dups.push(key.clone());
                    }
                } else {
                    keys.push(key.clone());
                }
            }
            Line::Blank => {}
        }
    }
    Ok(CapturePreview {
        entries: keys.len(),
        comments,
        bad,
        keys,
        dup_keys: dups,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub path: String,
    pub dir: Option<String>,
    pub name_guess: Option<String>,
    pub text: String,
}

fn read_env_file(path: &Path) -> R<PickedFile> {
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if bytes.len() > 2_000_000 {
        return Err("that file is larger than 2 MB — not an .env file?".into());
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let dir = path.parent().map(|p| p.to_string_lossy().to_string());
    // A .env usually lives in the project root, so the parent folder
    // name is a good default project name.
    let name_guess = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    Ok(PickedFile {
        path: path.to_string_lossy().to_string(),
        dir,
        name_guess,
        text,
    })
}

#[tauri::command]
pub async fn pick_env_file(app: AppHandle) -> R<Option<PickedFile>> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Choose a .env file to capture")
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("dialog failed: {e}"))?;

    match picked {
        None => Ok(None),
        Some(fp) => {
            let path = fp
                .into_path()
                .map_err(|e| format!("unsupported file location: {e}"))?;
            read_env_file(&path).map(Some)
        }
    }
}

/// Drag/drop is handled as a window event so the path never round-trips
/// through the webview: Rust reads the file and hands the UI a finished
/// payload. There is no path-taking IPC command for webview JavaScript
/// to call.
pub fn handle_drop(window: &tauri::Window, paths: &[PathBuf]) {
    let state = window.state::<AppState>();
    let unlocked = with_inner(&state, |inner| {
        Ok(matches!(inner.session, Session::Unlocked { .. }))
    })
    .unwrap_or(false);
    if !unlocked {
        return;
    }
    let Some(path) = paths.first() else { return };
    match read_env_file(path) {
        Ok(picked) => {
            let _ = window.emit("env-file-dropped", &picked);
        }
        Err(e) => {
            let _ = window.emit("env-drop-error", &e);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureArgs {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub path_hint: Option<String>,
    pub text: String,
    pub source_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub project_id: String,
    pub snapshot_id: String,
    pub entry_count: usize,
}

#[tauri::command]
pub fn capture(state: State<'_, AppState>, args: CaptureArgs) -> R<CaptureResult> {
    mutate(&state, |store| {
        let snapshot = Snapshot {
            id: store::new_id(),
            captured_at: store::now_iso(),
            via: if args.source_path.is_some() {
                "file".into()
            } else {
                "paste".into()
            },
            source_path: args.source_path.clone(),
            raw: args.text.clone(),
        };
        let snapshot_id = snapshot.id.clone();
        let entry_count = envfile::entry_count(&snapshot.raw);

        let hint = args
            .path_hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);

        let project_id = if let Some(id) = &args.project_id {
            let p = store
                .project_mut(id)
                .ok_or_else(|| "project not found".to_string())?;
            if let Some(h) = hint {
                p.path_hint = Some(h);
            }
            p.snapshots.push(snapshot);
            p.id.clone()
        } else {
            let name = args
                .project_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "give the project a name".to_string())?;
            if let Some(existing) = store.project_by_name(name).map(|p| p.id.clone()) {
                let p = store.project_mut(&existing).unwrap();
                if let Some(h) = hint {
                    p.path_hint = Some(h);
                }
                p.snapshots.push(snapshot);
                existing
            } else {
                let project = Project {
                    id: store::new_id(),
                    name: name.to_string(),
                    path_hint: hint,
                    created_at: store::now_iso(),
                    snapshots: vec![snapshot],
                };
                let id = project.id.clone();
                store.projects.push(project);
                id
            }
        };

        Ok(CaptureResult {
            project_id,
            snapshot_id,
            entry_count,
        })
    })
}

// -------------------------------------------------------- reveal & copy

/// The Windows clipboard is exclusive, and listeners (clipboard
/// history, sync services, managers) grab it the moment it changes.
/// Retrying with backoff for up to ~2s beats surfacing an error for
/// what is almost always a sub-second collision.
fn clipboard_retry<T>(what: &str, mut op: impl FnMut() -> Result<T, String>) -> R<T> {
    let mut last = String::new();
    for attempt in 0..8 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(60 * attempt));
        }
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(format!("could not {what} the clipboard: {last}"))
}

/// Raw clipboard write. On Windows the text is marked to stay out of
/// the clipboard history (Win+V) and the cross-device cloud clipboard —
/// both would silently retain (or upload) anything copied here long
/// after the paste.
fn write_clipboard(text: String) -> R<()> {
    clipboard_retry("write to", || {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let set = cb.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud()
        };
        set.text(text.clone()).map_err(|e| e.to_string())
    })
}

/// How long a copied secret stays on the clipboard before Envarsa
/// clears it — matches the 30s reveal auto-hide.
const CLIPBOARD_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Bumped on every secret copy; an expiring timer only acts if it is
/// still the latest, so re-copying restarts the 30s rather than
/// inheriting the old deadline.
static CLIPBOARD_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Copy a secret: write it, then clear it after CLIPBOARD_TTL — but
/// only if no newer copy was made and the clipboard still holds exactly
/// what was copied, so nothing of anyone else's is ever clobbered.
fn copy_secret_to_clipboard(text: String) -> R<()> {
    use std::sync::atomic::Ordering;
    write_clipboard(text.clone())?;
    let generation = CLIPBOARD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(CLIPBOARD_TTL);
        if CLIPBOARD_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Ok(mut cb) = arboard::Clipboard::new() else {
            return;
        };
        if cb.get_text().map(|t| t == text).unwrap_or(false) {
            let _ = cb.clear();
        }
    });
    Ok(())
}

fn line_at(store: &Store, project_id: &str, snapshot_id: &str, idx: usize) -> R<Line> {
    let project = store
        .project(project_id)
        .ok_or_else(|| "project not found".to_string())?;
    let snapshot = project
        .snapshot(snapshot_id)
        .ok_or_else(|| "snapshot not found".to_string())?;
    envfile::parse(&snapshot.raw)
        .into_iter()
        .nth(idx)
        .ok_or_else(|| "no such line".to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedValue {
    pub key: String,
    pub value: String,
}

/// Entries reveal their value; bad (unparseable) lines reveal their raw
/// text — those are masked in the listing too, since a malformed line
/// is as likely as any to hold a secret.
#[tauri::command]
pub fn reveal_value(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    idx: usize,
) -> R<RevealedValue> {
    with_store(&state, |store| {
        match line_at(store, &project_id, &snapshot_id, idx)? {
            Line::Entry { key, value, .. } => Ok(RevealedValue { key, value }),
            Line::Bad(raw) => Ok(RevealedValue {
                key: String::new(),
                value: raw,
            }),
            _ => Err("that line has no value to reveal".into()),
        }
    })
}

/// Copies a single value Rust → OS clipboard; the value never transits
/// the webview.
#[tauri::command]
pub fn copy_value(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    idx: usize,
) -> R<String> {
    with_store(&state, |store| {
        match line_at(store, &project_id, &snapshot_id, idx)? {
            Line::Entry { key, value, .. } => {
                copy_secret_to_clipboard(value)?;
                Ok(key)
            }
            _ => Err("that line is not an entry".into()),
        }
    })
}

/// Copies the whole snapshot block (raw bytes, exactly as captured).
#[tauri::command]
pub fn copy_block(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<usize> {
    with_store(&state, |store| {
        let project = store
            .project(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let snapshot = project
            .snapshot(&snapshot_id)
            .ok_or_else(|| "snapshot not found".to_string())?;
        copy_secret_to_clipboard(snapshot.raw.clone())?;
        Ok(envfile::entry_count(&snapshot.raw))
    })
}

// ---------------------------------------------------------------- export

/// Export = save dialog; the user places the file themselves. Envarsa
/// never writes into a project tree on its own.
#[tauri::command]
pub async fn export_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<Option<String>> {
    let (raw, suggested) = with_store(&state, |store| {
        let project = store
            .project(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let snapshot = project
            .snapshot(&snapshot_id)
            .ok_or_else(|| "snapshot not found".to_string())?;
        Ok((snapshot.raw.clone(), format!("{}.env", project.name)))
    })?;

    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Export snapshot as .env")
            .set_file_name(&suggested)
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("dialog failed: {e}"))?;

    match picked {
        None => Ok(None),
        Some(fp) => {
            let path = fp
                .into_path()
                .map_err(|e| format!("unsupported file location: {e}"))?;
            fs::write(&path, raw.as_bytes())
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
    }
}

/// Test hook: export without a dialog. Only honored when the selftest
/// env var is set, so the "user places the file" rule can't be bypassed
/// in normal runs.
#[tauri::command]
pub fn export_to_path(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    path: String,
) -> R<()> {
    if !selftest_active() {
        return Err("export_to_path is a selftest-only command".into());
    }
    with_store(&state, |store| {
        let project = store
            .project(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let snapshot = project
            .snapshot(&snapshot_id)
            .ok_or_else(|| "snapshot not found".to_string())?;
        fs::write(&path, snapshot.raw.as_bytes()).map_err(|e| e.to_string())
    })
}

// ----------------------------------------------------- store export

/// Serialize the live store, optionally encrypting the copy with a
/// transport passphrase (independent of the at-rest one).
fn store_copy_bytes(state: &State<'_, AppState>, passphrase: Option<&str>) -> R<Vec<u8>> {
    if let Some(p) = passphrase {
        if p.chars().count() < 8 {
            return Err("use at least 8 characters".into());
        }
    }
    let bytes = with_store(state, |store| Ok(store::serialize_store(store)))?;
    match passphrase {
        Some(p) => crypto::encrypt(&bytes, p),
        None => Ok(bytes),
    }
}

/// Save a copy of the whole library wherever the user chooses — the
/// "hand it to another machine" path, without digging through AppData.
/// The live store and its at-rest encryption are untouched.
#[tauri::command]
pub async fn export_store(
    app: AppHandle,
    state: State<'_, AppState>,
    passphrase: Option<String>,
) -> R<Option<String>> {
    let bytes = store_copy_bytes(&state, passphrase.as_deref())?;
    let suggested = format!("envarsa-{}.store", chrono::Local::now().format("%Y-%m-%d"));

    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Export a copy of the store")
            .set_file_name(&suggested)
            .add_filter("Envarsa store", &["store"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("dialog failed: {e}"))?;

    match picked {
        None => Ok(None),
        Some(fp) => {
            let path = fp
                .into_path()
                .map_err(|e| format!("unsupported file location: {e}"))?;
            fs::write(&path, &bytes)
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
    }
}

/// Test hook: export the store copy without a dialog. Selftest-only.
#[tauri::command]
pub fn export_store_to_path(
    state: State<'_, AppState>,
    path: String,
    passphrase: Option<String>,
) -> R<()> {
    if !selftest_active() {
        return Err("export_store_to_path is a selftest-only command".into());
    }
    let bytes = store_copy_bytes(&state, passphrase.as_deref())?;
    fs::write(&path, &bytes).map_err(|e| e.to_string())
}

// ----------------------------------------------------- project editing

#[tauri::command]
pub fn rename_project(state: State<'_, AppState>, project_id: String, name: String) -> R<()> {
    mutate(&state, |store| {
        let name = name.trim();
        if name.is_empty() {
            return Err("the name cannot be empty".into());
        }
        if let Some(other) = store.project_by_name(name) {
            if other.id != project_id {
                return Err(format!("a project named \"{}\" already exists", other.name));
            }
        }
        let p = store
            .project_mut(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        p.name = name.to_string();
        Ok(())
    })
}

#[tauri::command]
pub fn set_path_hint(state: State<'_, AppState>, project_id: String, path_hint: String) -> R<()> {
    mutate(&state, |store| {
        let p = store
            .project_mut(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let trimmed = path_hint.trim();
        p.path_hint = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        Ok(())
    })
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, project_id: String) -> R<()> {
    mutate(&state, |store| {
        let before = store.projects.len();
        store.projects.retain(|p| p.id != project_id);
        if store.projects.len() == before {
            return Err("project not found".into());
        }
        Ok(())
    })
}

/// Re-adds an older snapshot as the newest one ("bring this back").
#[tauri::command]
pub fn promote_snapshot(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<String> {
    mutate(&state, |store| {
        let p = store
            .project_mut(&project_id)
            .ok_or_else(|| "project not found".to_string())?;
        let src = p
            .snapshot(&snapshot_id)
            .ok_or_else(|| "snapshot not found".to_string())?;
        let snapshot = Snapshot {
            id: store::new_id(),
            captured_at: store::now_iso(),
            via: "restore".into(),
            source_path: src.source_path.clone(),
            raw: src.raw.clone(),
        };
        let id = snapshot.id.clone();
        p.snapshots.push(snapshot);
        Ok(id)
    })
}

// ------------------------------------------------------------ protection

#[tauri::command]
pub fn enable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    if passphrase.chars().count() < 8 {
        return Err("use at least 8 characters".into());
    }
    with_inner(&state, |inner| {
        let path = inner.store_path.clone();
        match &mut inner.session {
            Session::Unlocked {
                store,
                passphrase: current,
            } => {
                if current.is_some() {
                    return Err("encryption is already enabled".into());
                }
                *current = Some(passphrase);
                let pass = current.clone();
                if let Err(e) = store::save(store, &path, pass.as_deref()) {
                    *current = None; // the file on disk is still plaintext
                    return Err(e);
                }
                // The save preserved the pre-encryption bytes as `.bak`;
                // rewrite it so no plaintext copy outlives the transition.
                store::align_backup(&path).map_err(|e| {
                    format!("the store is encrypted, but the old plaintext backup survived — {e}")
                })
            }
            _ => Err("unlock the store first".into()),
        }
    })
}

#[tauri::command]
pub fn change_passphrase(
    state: State<'_, AppState>,
    current: String,
    new_passphrase: String,
) -> R<()> {
    if new_passphrase.chars().count() < 8 {
        return Err("use at least 8 characters".into());
    }
    with_inner(&state, |inner| {
        let path = inner.store_path.clone();
        match &mut inner.session {
            Session::Unlocked {
                store,
                passphrase: Some(held),
            } => {
                if *held != current {
                    return Err("the current passphrase is not right".into());
                }
                let old = std::mem::replace(held, new_passphrase);
                if let Err(e) = store::save(store, &path, Some(held.clone()).as_deref()) {
                    *held = old; // the file on disk still uses the old passphrase
                    return Err(e);
                }
                // Don't leave a backup that the old passphrase still opens.
                store::align_backup(&path).map_err(|e| {
                    format!("the passphrase was changed, but the backup still uses the old one — {e}")
                })
            }
            _ => Err("encryption is not enabled".into()),
        }
    })
}

#[tauri::command]
pub fn disable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    with_inner(&state, |inner| {
        let path = inner.store_path.clone();
        match &mut inner.session {
            Session::Unlocked {
                store,
                passphrase: held @ Some(_),
            } => {
                if held.as_deref() != Some(passphrase.as_str()) {
                    return Err("the passphrase is not right".into());
                }
                let old = held.take();
                if let Err(e) = store::save(store, &path, None) {
                    *held = old; // the file on disk is still encrypted
                    return Err(e);
                }
                // Keep the backup in step with the live store's
                // protection state, so a later restore can't silently
                // re-encrypt (or vice versa).
                store::align_backup(&path).map_err(|e| {
                    format!("the store is decrypted, but the backup could not be rewritten — {e}")
                })
            }
            _ => Err("encryption is not enabled".into()),
        }
    })
}

// ------------------------------------------------------------ store file

#[tauri::command]
pub fn reveal_store(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    with_inner(&state, |inner| {
        app.opener()
            .reveal_item_in_dir(&inner.store_path)
            .map_err(|e| format!("could not open the file location: {e}"))
    })
}

/// Move the store file somewhere the user chooses (e.g. a folder they
/// sync themselves). Manual, user-owned portability.
#[tauri::command]
pub async fn relocate_store(app: AppHandle, state: State<'_, AppState>) -> R<Option<String>> {
    let (old_path, env_override) = with_inner(&state, |inner| {
        Ok((inner.store_path.clone(), inner.env_override))
    })?;
    if env_override {
        return Err(
            "the store location is currently forced by ENVARSA_STORE_PATH — unset it first"
                .into(),
        );
    }

    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Move the store file")
            .set_file_name("envarsa.store")
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("dialog failed: {e}"))?;

    let Some(fp) = picked else { return Ok(None) };
    let new_path = fp
        .into_path()
        .map_err(|e| format!("unsupported file location: {e}"))?;
    if new_path == old_path {
        return Ok(Some(new_path.to_string_lossy().to_string()));
    }

    with_inner(&state, |inner| {
        if let Some(dir) = new_path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("could not create folder: {e}"))?;
        }
        fs::copy(&old_path, &new_path).map_err(|e| format!("could not copy the store: {e}"))?;
        inner.config.store_path = Some(new_path.to_string_lossy().to_string());
        state::save_config(&inner.config_path, &inner.config)?;
        inner.store_path = new_path.clone();
        // Best effort: tidy up the old location.
        let _ = fs::remove_file(&old_path);
        let _ = fs::remove_file(store::backup_path(&old_path));
        Ok(Some(new_path.to_string_lossy().to_string()))
    })
}

// --------------------------------------------------------------- updates

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

/// The manual "Check for updates" button — the loud path: failures come
/// back as errors for the settings modal to show inline. The request
/// itself lives in update.rs, the app's entire network surface.
#[tauri::command]
pub async fn check_for_updates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> R<UpdateCheckResult> {
    if selftest_active() {
        return Err("update checks are disabled during selftest".into());
    }
    let latest = tauri::async_runtime::spawn_blocking(crate::update::fetch_latest_version)
        .await
        .map_err(|e| format!("update check failed: {e}"))??;
    let current = app.package_info().version.clone();
    let newer = latest > current;
    // Best effort: a config-write failure must not eat a good answer.
    // A manual check legitimately postpones the next automatic one.
    let _ = with_inner(&state, |inner| {
        inner.config.last_update_check = Some(chrono::Utc::now().timestamp());
        inner.config.available_version = newer.then(|| latest.to_string());
        let _ = state::save_config(&inner.config_path, &inner.config);
        Ok(())
    });
    Ok(UpdateCheckResult {
        current: current.to_string(),
        latest: latest.to_string(),
        update_available: newer,
    })
}

#[tauri::command]
pub fn set_auto_update_check(state: State<'_, AppState>, enabled: bool) -> R<()> {
    with_inner(&state, |inner| {
        inner.config.auto_update_check = enabled;
        // This save failure does surface — the UI reverts the toggle.
        state::save_config(&inner.config_path, &inner.config)
    })
}

/// Opens the releases page in the default browser. The URL is a
/// compile-time constant — nothing fetched ever becomes a link, and the
/// webview holds no URL-opening primitive of its own.
#[tauri::command]
pub fn open_releases_page(app: AppHandle) -> R<()> {
    app.opener()
        .open_url(crate::update::RELEASES_PAGE_URL, None::<&str>)
        .map_err(|e| format!("could not open the releases page: {e}"))
}

// ------------------------------------------------------- store import
//
// Importing merges another store's projects into the current library;
// the live store file stays exactly where it is. (Its predecessor,
// "adopt", switched the app onto the picked file — which quietly made
// e.g. your Downloads folder the live store location.)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProjectPreview {
    pub name: String,
    pub snapshot_count: usize,
    pub entry_count: usize,
    pub latest_captured_at: Option<String>,
    /// The existing project this one collides with (by name), if any.
    pub conflicts_with: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Opaque handle for the picked file; apply must present it back.
    pub token: String,
    /// Display only — the webview never sends a path back.
    pub path: String,
    pub encrypted: bool,
    /// False when the file is encrypted and no passphrase was given —
    /// `projects` is empty then and the UI asks for the passphrase.
    pub unlocked: bool,
    pub projects: Vec<ImportProjectPreview>,
}

/// Read and parse a store file someone wants to import. Returns
/// (encrypted, parsed); parsed is None when the file is encrypted and
/// no passphrase was supplied.
fn read_import_file(path: &Path, passphrase: Option<&str>) -> R<(bool, Option<Store>)> {
    let bytes = fs::read(path).map_err(|e| format!("could not read that file: {e}"))?;
    if crypto::is_encrypted(&bytes) {
        match passphrase {
            None => Ok((true, None)),
            Some(p) => {
                let plain = crypto::decrypt(&bytes, p)?;
                Ok((true, Some(store::parse_store(&plain)?)))
            }
        }
    } else {
        let s = store::parse_store(&bytes)
            .map_err(|e| format!("that file is not an Envarsa store: {e}"))?;
        Ok((false, Some(s)))
    }
}

fn guard_not_live_store(state: &State<'_, AppState>, path: &Path) -> R<()> {
    let live = with_inner(state, |inner| Ok(inner.store_path.clone()))?;
    let same = match (fs::canonicalize(path), fs::canonicalize(&live)) {
        (Ok(a), Ok(b)) => a == b,
        _ => path == live,
    };
    if same {
        return Err("that file is the store Envarsa is already using".into());
    }
    Ok(())
}

/// Remember a picked import file and hand back the token for it. The
/// path stays on the Rust side; the webview only ever sees the token.
fn stage_import(state: &State<'_, AppState>, path: PathBuf) -> R<String> {
    let token = store::new_id();
    with_inner(state, |inner| {
        inner.pending_import = Some(state::PendingImport {
            token: token.clone(),
            path,
        });
        Ok(())
    })?;
    Ok(token)
}

fn pending_import_path(state: &State<'_, AppState>, token: &str) -> R<PathBuf> {
    with_inner(state, |inner| match &inner.pending_import {
        Some(p) if p.token == token => Ok(p.path.clone()),
        _ => Err("that import is no longer pending — pick the store file again".into()),
    })
}

#[tauri::command]
pub async fn pick_import_store(app: AppHandle, state: State<'_, AppState>) -> R<Option<String>> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .set_title("Import an Envarsa store")
            .add_filter("Envarsa store", &["store", "bak"])
            .add_filter("All files", &["*"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("dialog failed: {e}"))?;
    match picked {
        None => Ok(None),
        Some(fp) => {
            let path = fp
                .into_path()
                .map_err(|e| format!("unsupported file location: {e}"))?;
            stage_import(&state, path).map(Some)
        }
    }
}

/// First look at a store file before importing: is it encrypted, what
/// projects does it hold, and which of them collide with ours. Only
/// names and counts cross to the UI — never values.
#[tauri::command]
pub fn inspect_import(
    state: State<'_, AppState>,
    token: String,
    passphrase: Option<String>,
) -> R<ImportPreview> {
    let path = pending_import_path(&state, &token)?;
    guard_not_live_store(&state, &path)?;
    let (encrypted, parsed) = read_import_file(&path, passphrase.as_deref())?;
    let display = path.to_string_lossy().to_string();
    let Some(incoming) = parsed else {
        return Ok(ImportPreview {
            token,
            path: display,
            encrypted,
            unlocked: false,
            projects: Vec::new(),
        });
    };
    with_store(&state, |store| {
        let projects = incoming
            .projects
            .iter()
            .map(|ip| ImportProjectPreview {
                name: ip.name.clone(),
                snapshot_count: ip.snapshots.len(),
                entry_count: latest_effective(ip).len(),
                latest_captured_at: ip.latest().map(|s| s.captured_at.clone()),
                conflicts_with: store.project_by_name(&ip.name).map(|e| e.name.clone()),
            })
            .collect();
        Ok(ImportPreview {
            token: token.clone(),
            path: display.clone(),
            encrypted,
            unlocked: true,
            projects,
        })
    })
}

/// Merge a previewed store file into the library, one decision per
/// incoming project (add / replace / rename / skip). The file is
/// re-read and re-validated here — nothing is trusted from the
/// preview round-trip.
#[tauri::command]
pub fn apply_import(
    state: State<'_, AppState>,
    token: String,
    passphrase: Option<String>,
    decisions: Vec<store::ImportDecision>,
) -> R<store::ImportSummary> {
    let path = pending_import_path(&state, &token)?;
    guard_not_live_store(&state, &path)?;
    let (_, parsed) = read_import_file(&path, passphrase.as_deref())?;
    let incoming =
        parsed.ok_or_else(|| "that store is encrypted — its passphrase is needed".to_string())?;
    mutate(&state, |store| store::merge_import(store, incoming, &decisions))
}

/// Restoring is only offered (and only allowed) when the store cannot
/// be loaded — on a healthy session it would be a silent rollback, and
/// on an encrypted one a possible downgrade to whatever the backup
/// holds.
#[tauri::command]
pub fn restore_backup(app: AppHandle, state: State<'_, AppState>) -> R<StatusPayload> {
    with_inner(&state, |inner| {
        if !matches!(inner.session, Session::Corrupt { .. }) {
            return Err("the store loaded fine — restoring the backup is only for when it cannot be read".into());
        }
        let bak = store::backup_path(&inner.store_path);
        if !bak.exists() {
            return Err("no backup file exists next to the store".into());
        }
        fs::copy(&bak, &inner.store_path)
            .map_err(|e| format!("could not restore the backup: {e}"))?;
        inner.session = state::init_session(&inner.store_path);
        Ok(status_of(&app, inner))
    })
}

// ------------------------------------------------------------------ misc

#[tauri::command]
pub fn ui_log(level: String, message: String) {
    println!("[ui:{level}] {message}");
}

#[tauri::command]
pub fn selftest_enabled() -> bool {
    selftest_active()
}

#[tauri::command]
pub fn selftest_read_clipboard() -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    clipboard_retry("read", || {
        arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .map_err(|e| e.to_string())
    })
}

/// No auto-clear here — the selftest uses this to put the user's
/// original clipboard back when it finishes.
#[tauri::command]
pub fn selftest_set_clipboard(text: String) -> R<()> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    write_clipboard(text)
}

/// Test hook: read a file back (lossy text), e.g. an export or the
/// `.bak` sibling, to assert on the bytes Envarsa wrote. Selftest-only.
#[tauri::command]
pub fn selftest_read_file(path: String) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Test hook: stage an import by path, standing in for the picker
/// dialog (which needs a human). Selftest-only.
#[tauri::command]
pub fn selftest_stage_import(state: State<'_, AppState>, path: String) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    stage_import(&state, PathBuf::from(path))
}

#[tauri::command]
pub fn selftest_done(app: AppHandle, passed: usize, failed: usize, report: String) {
    if !selftest_active() {
        return; // not an exit lever for normal runs
    }
    use std::io::Write as _;
    println!("SELFTEST REPORT\n{report}\nSELFTEST: {passed} passed, {failed} failed");
    let _ = std::io::stdout().flush();
    app.exit(if failed == 0 { 0 } else { 1 });
}
