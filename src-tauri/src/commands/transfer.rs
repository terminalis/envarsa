use super::{dialog_path, latest_effective, mutate, with_store, R};
use crate::crypto;
use crate::state::{self, AppState};
use crate::store::{self, Opened, Store};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

// ------------------------------------------------------------ store file

#[tauri::command]
pub fn reveal_store(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    state.with(|inner| {
        app.opener()
            .reveal_item_in_dir(&inner.store_path)
            .map_err(|e| format!("could not open the file location: {e}"))
    })
}

/// Move the store file somewhere the user chooses (e.g. a folder they
/// sync themselves). Manual, user-owned portability.
#[tauri::command]
pub async fn relocate_store(app: AppHandle, state: State<'_, AppState>) -> R<Option<String>> {
    let (old_path, env_override) =
        state.with(|inner| Ok((inner.store_path.clone(), inner.env_override)))?;
    if env_override {
        return Err(
            "the store location is currently forced by ENVARSA_STORE_PATH — unset it first".into(),
        );
    }

    let Some(new_path) = dialog_path(&app, |d| {
        d.set_title("Move the store file")
            .set_file_name("envarsa.store")
            .blocking_save_file()
    })
    .await?
    else {
        return Ok(None);
    };
    if new_path == old_path {
        return Ok(Some(new_path.to_string_lossy().to_string()));
    }

    state.with(|inner| {
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
    match store::open(&bytes, passphrase) {
        Ok(Opened::Plain(s)) => Ok((false, Some(s))),
        Ok(Opened::Encrypted(s)) => Ok((true, Some(s))),
        Ok(Opened::NeedsPassphrase) => Ok((true, None)),
        Err(e) if crypto::is_encrypted(&bytes) => Err(e),
        Err(e) => Err(format!("that file is not an Envarsa store: {e}")),
    }
}

fn guard_not_live_store(state: &State<'_, AppState>, path: &Path) -> R<()> {
    let live = state.with(|inner| Ok(inner.store_path.clone()))?;
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
pub(crate) fn stage_import(state: &State<'_, AppState>, path: PathBuf) -> R<String> {
    state.with(|inner| Ok(inner.stage_import(path)))
}

fn pending_import_path(state: &State<'_, AppState>, token: &str) -> R<PathBuf> {
    state.with(|inner| {
        inner
            .pending_import(token)
            .map(Path::to_path_buf)
            .ok_or_else(|| "that import is no longer pending — pick the store file again".into())
    })
}

#[tauri::command]
pub async fn pick_import_store(app: AppHandle, state: State<'_, AppState>) -> R<Option<String>> {
    dialog_path(&app, |d| {
        d.set_title("Import an Envarsa store")
            .add_filter("Envarsa store", &["store", "bak"])
            .add_filter("All files", &["*"])
            .blocking_pick_file()
    })
    .await?
    .map(|path| stage_import(&state, path))
    .transpose()
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
    mutate(&state, |store| {
        store::merge_import(store, incoming, &decisions)
    })
}
