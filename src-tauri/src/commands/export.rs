use super::session::check_passphrase;
use super::{dialog_path, selftest_active, with_store, R};
use crate::crypto;
use crate::envfile::{self, Line};
use crate::state::AppState;
use crate::store::{self, Store};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

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
        let (project, snapshot) = store.find(&project_id, Some(&snapshot_id))?;
        Ok((snapshot.raw.clone(), format!("{}.env", project.name)))
    })?;

    let Some(path) = dialog_path(&app, move |d| {
        d.set_title("Export snapshot as .env")
            .set_file_name(&suggested)
            .blocking_save_file()
    })
    .await?
    else {
        return Ok(None);
    };
    fs::write(&path, raw.as_bytes())
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().to_string()))
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
        let (_, snapshot) = store.find(&project_id, Some(&snapshot_id))?;
        fs::write(&path, snapshot.raw.as_bytes()).map_err(|e| e.to_string())
    })
}

// ----------------------------------------------------- store export

/// Serialize the live store, optionally encrypting the copy with a
/// transport passphrase (independent of the at-rest one).
fn store_copy_bytes(state: &State<'_, AppState>, passphrase: Option<&str>) -> R<Vec<u8>> {
    if let Some(p) = passphrase {
        check_passphrase(p)?;
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

    let Some(path) = dialog_path(&app, move |d| {
        d.set_title("Export a copy of the store")
            .set_file_name(&suggested)
            .add_filter("Envarsa store", &["store"])
            .blocking_save_file()
    })
    .await?
    else {
        return Ok(None);
    };
    fs::write(&path, &bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().to_string()))
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

// ------------------------------------------------------ write .env.local
//
// The one place Envarsa writes into a project tree. The target is always
// a `.env*.local` (gitignored), never an example file (git-committed —
// secrets would leak). The guard is enforced here on the final path; the
// destination is staged behind an opaque token, so the webview never
// supplies a path. The store is never touched — this is an export.

/// "writable" | "example" | "other" — for the UI badge.
fn class_str(path: &Path) -> &'static str {
    use crate::envpath::NameClass;
    match crate::envpath::classify_name(path) {
        NameClass::WritableLocal => "writable",
        NameClass::ExampleFamily => "example",
        NameClass::Other => "other",
    }
}

/// The last check before any bytes are written. Run on the final resolved
/// path, regardless of how it was chosen.
fn guard_writable_local(path: &Path) -> R<()> {
    use crate::envpath::NameClass;
    match crate::envpath::classify_name(path) {
        NameClass::WritableLocal => Ok(()),
        NameClass::ExampleFamily => Err(
            "refusing to write into an example file — .env.example/.sample/.template/.dist are \
             committed to git, so secrets would leak. Write to a .env.local instead."
                .into(),
        ),
        NameClass::Other => Err(
            "Envarsa only writes to the .env*.local family (.env.local, .env.development.local, …), \
             which is gitignored."
                .into(),
        ),
    }
}

pub(crate) fn stage_write(
    state: &State<'_, AppState>,
    path: PathBuf,
    template: Option<String>,
) -> R<String> {
    state.with(|inner| Ok(inner.stage_write(path, template)))
}

fn pending_write(state: &State<'_, AppState>, token: &str) -> R<(PathBuf, Option<String>)> {
    state.with(|inner| {
        inner
            .pending_write(token)
            .map(|p| (p.path.clone(), p.template.clone()))
            .ok_or_else(|| "that write is no longer staged — choose the location again".into())
    })
}

fn clear_pending_write(state: &State<'_, AppState>) {
    let _ = state.with(|inner| {
        inner.clear_pending_writes();
        Ok(())
    });
}

fn snapshot_raw(store: &Store, project_id: &str, snapshot_id: &str) -> R<String> {
    Ok(store.find(project_id, Some(snapshot_id))?.1.raw.clone())
}

/// Read a `.env.local` to merge into. Missing → empty (nothing to keep).
fn read_target_text(path: &Path) -> R<String> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > 2_000_000 {
                return Err("that file is larger than 2 MB — refusing to merge".into());
            }
            Ok(String::from_utf8_lossy(&bytes).to_string())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("could not read {}: {e}", path.display())),
    }
}

/// Partition keys for the preview: which source keys are appended, which
/// target keys are substituted, and which are blanked (scaffold) or kept
/// (merge). Names only — values never cross here.
fn diff_keys(
    target_lines: &[Line],
    source: &[(String, String)],
    empty_out: bool,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    use std::collections::HashSet;
    let source_keys: HashSet<&str> = source.iter().map(|(k, _)| k.as_str()).collect();
    let mut target_keys: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for l in target_lines {
        if let Line::Entry { key, .. } = l {
            if seen.insert(key.as_str()) {
                target_keys.push(key.as_str());
            }
        }
    }
    let mut substituted = Vec::new();
    let mut emptied = Vec::new();
    let mut kept = Vec::new();
    for k in &target_keys {
        if source_keys.contains(k) {
            substituted.push((*k).to_string());
        } else if empty_out {
            emptied.push((*k).to_string());
        } else {
            kept.push((*k).to_string());
        }
    }
    let added: Vec<String> = source
        .iter()
        .filter(|(k, _)| !seen.contains(k.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    (added, substituted, emptied, kept)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTarget {
    /// Opaque handle; the write must present it back. Path stays Rust-side.
    pub token: String,
    /// Display only — the webview never sends a path back.
    pub path: String,
    /// The target's directory, to seed a "change location" dialog.
    pub dir: String,
    pub class: String,
    pub exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePreview {
    pub result_entry_count: usize,
    pub added: Vec<String>,
    pub substituted: Vec<String>,
    pub emptied: Vec<String>,
    pub kept: Vec<String>,
    /// A guard refusal, so the UI can show it without throwing.
    pub blocked: Option<String>,
    pub mode: String,
}

/// Stage the default target: `<remembered dir>/.env.local`. The dir is
/// the snapshot's source directory, else the project's path hint — both
/// user-supplied, neither from the webview.
#[tauri::command]
pub fn stage_write_target(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<WriteTarget> {
    let dir = with_store(&state, |store| {
        let (project, snapshot) = store.find(&project_id, Some(&snapshot_id))?;
        let from_source = snapshot
            .source_path
            .as_deref()
            .map(PathBuf::from)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let from_hint = project.path_hint.as_deref().map(PathBuf::from);
        from_source.or(from_hint).ok_or_else(|| {
            "no remembered directory for this project — use “Change location” to choose where to write"
                .to_string()
        })
    })?;
    let path = dir.join(".env.local");
    let target = WriteTarget {
        token: stage_write(&state, path.clone(), None)?,
        class: class_str(&path).to_string(),
        exists: path.exists(),
        dir: dir.to_string_lossy().to_string(),
        path: path.to_string_lossy().to_string(),
    };
    Ok(target)
}

/// Redirect the target via a save dialog. `suggested_dir` only seeds the
/// dialog's starting folder; the staged path is the user's actual pick.
#[tauri::command]
pub async fn pick_write_target(
    app: AppHandle,
    state: State<'_, AppState>,
    suggested_dir: Option<String>,
) -> R<Option<WriteTarget>> {
    let picked = dialog_path(&app, move |d| {
        let mut d = d.set_title("Write .env.local").set_file_name(".env.local");
        if let Some(dir) = suggested_dir.as_deref() {
            d = d.set_directory(dir);
        }
        d.blocking_save_file()
    })
    .await?;

    match picked {
        None => Ok(None),
        Some(path) => {
            let dir = path
                .parent()
                .map(|d| d.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(Some(WriteTarget {
                token: stage_write(&state, path.clone(), None)?,
                class: class_str(&path).to_string(),
                exists: path.exists(),
                dir,
                path: path.to_string_lossy().to_string(),
            }))
        }
    }
}

fn build_write_text(path: &Path, mode: &str, raw: &str) -> R<(String, WritePreview)> {
    let snap_lines = envfile::parse(raw);
    let source = envfile::effective_entries(&snap_lines);
    if mode == "merge" && path.exists() {
        let target_lines = envfile::parse(&read_target_text(path)?);
        let (added, substituted, emptied, kept) = diff_keys(&target_lines, &source, false);
        let text = envfile::merge(&target_lines, &source, envfile::AbsentPolicy::KeepTarget);
        let count = envfile::entry_count(&text);
        Ok((
            text,
            WritePreview {
                result_entry_count: count,
                added,
                substituted,
                emptied,
                kept,
                blocked: None,
                mode: mode.to_string(),
            },
        ))
    } else {
        // fresh / overwrite: the snapshot's own re-serialized lines.
        let text = envfile::serialize_lines(&snap_lines);
        let keys: Vec<String> = source.iter().map(|(k, _)| k.clone()).collect();
        let count = envfile::entry_count(&text);
        Ok((
            text,
            WritePreview {
                result_entry_count: count,
                added: keys,
                substituted: Vec::new(),
                emptied: Vec::new(),
                kept: Vec::new(),
                blocked: None,
                mode: if path.exists() { "overwrite" } else { "fresh" }.to_string(),
            },
        ))
    }
}

#[tauri::command]
pub fn preview_write(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
    mode: String,
) -> R<WritePreview> {
    let (path, _) = pending_write(&state, &token)?;
    let blocked = guard_writable_local(&path).err();
    with_store(&state, |store| {
        let raw = snapshot_raw(store, &project_id, &snapshot_id)?;
        let (_, mut preview) = build_write_text(&path, &mode, &raw)?;
        preview.blocked = blocked.clone();
        Ok(preview)
    })
}

#[tauri::command]
pub fn write_env_local(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
    mode: String,
) -> R<String> {
    let (path, _) = pending_write(&state, &token)?;
    guard_writable_local(&path)?;
    let text = with_store(&state, |store| {
        let raw = snapshot_raw(store, &project_id, &snapshot_id)?;
        Ok(build_write_text(&path, &mode, &raw)?.0)
    })?;
    store::write_atomic(&path, text.as_bytes())?;
    clear_pending_write(&state);
    Ok(path.to_string_lossy().to_string())
}

// --- import a .env.example as a scaffold, write .env.local beside it ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleStaged {
    pub token: String,
    pub example_name: String,
    /// Display only — the staged output path (`<example dir>/.env.local`).
    pub out_path: String,
    pub out_class: String,
    pub out_exists: bool,
    pub example_keys: Vec<String>,
    pub example_comments: usize,
}

/// Pick a `.env.example` to use as a template. Only its text is read; the
/// staged write target is `<example dir>/.env.local` — the example path
/// is never staged for writing.
#[tauri::command]
pub async fn pick_example_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> R<Option<ExampleStaged>> {
    let Some(path) = dialog_path(&app, |d| {
        d.set_title("Choose a .env.example to use as a template")
            .blocking_pick_file()
    })
    .await?
    else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if bytes.len() > 2_000_000 {
        return Err("that file is larger than 2 MB — not an .env example?".into());
    }
    let template = String::from_utf8_lossy(&bytes).to_string();
    let example_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let out_path = path
        .parent()
        .map(|d| d.join(".env.local"))
        .ok_or_else(|| "that file has no parent directory".to_string())?;
    let lines = envfile::parse(&template);
    let example_keys: Vec<String> = envfile::effective_entries(&lines)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let example_comments = lines
        .iter()
        .filter(|l| matches!(l, Line::Comment(_)))
        .count();
    Ok(Some(ExampleStaged {
        token: stage_write(&state, out_path.clone(), Some(template))?,
        out_class: class_str(&out_path).to_string(),
        out_exists: out_path.exists(),
        out_path: out_path.to_string_lossy().to_string(),
        example_name,
        example_keys,
        example_comments,
    }))
}

#[tauri::command]
pub fn preview_example_write(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
) -> R<WritePreview> {
    let (path, template) = pending_write(&state, &token)?;
    let template =
        template.ok_or_else(|| "that staged write has no example template".to_string())?;
    let blocked = guard_writable_local(&path).err();
    with_store(&state, |store| {
        let raw = snapshot_raw(store, &project_id, &snapshot_id)?;
        let source = envfile::effective_entries(&envfile::parse(&raw));
        let target_lines = envfile::parse(&template);
        let (added, substituted, emptied, kept) = diff_keys(&target_lines, &source, true);
        let text = envfile::merge(&target_lines, &source, envfile::AbsentPolicy::EmptyOut);
        Ok(WritePreview {
            result_entry_count: envfile::entry_count(&text),
            added,
            substituted,
            emptied,
            kept,
            blocked: blocked.clone(),
            mode: "example".to_string(),
        })
    })
}

#[tauri::command]
pub fn write_example_scaffold(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
) -> R<String> {
    let (path, template) = pending_write(&state, &token)?;
    let template =
        template.ok_or_else(|| "that staged write has no example template".to_string())?;
    guard_writable_local(&path)?;
    let text = with_store(&state, |store| {
        let raw = snapshot_raw(store, &project_id, &snapshot_id)?;
        let source = envfile::effective_entries(&envfile::parse(&raw));
        Ok(envfile::merge(
            &envfile::parse(&template),
            &source,
            envfile::AbsentPolicy::EmptyOut,
        ))
    })?;
    store::write_atomic(&path, text.as_bytes())?;
    clear_pending_write(&state);
    Ok(path.to_string_lossy().to_string())
}
