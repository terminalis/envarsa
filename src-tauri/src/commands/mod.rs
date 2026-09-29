//! The IPC surface. The webview can only reach these commands; every
//! OS interaction (file dialogs, clipboard, shell reveal) stays in the
//! Rust core. Values cross into the webview only on an explicit
//! per-value reveal, or into the editor when the user opens it —
//! listing a project sends keys and structure, never values. "Copy"
//! hands a value straight from the core to the OS clipboard without it
//! ever transiting the UI. Paths only travel the other way, for
//! display: a picked file comes back as an opaque token, and later
//! commands take the token. These are PATHS-STAY-IN-CORE and
//! VALUES-ON-REVEAL in ARCHITECTURE.md.

pub(crate) mod export;
pub(crate) mod library;
pub(crate) mod secrets;
pub(crate) mod session;
pub(crate) mod transfer;

#[cfg(test)]
mod tests;

use crate::envfile;
use crate::state::AppState;
use crate::store::{self, Project, Store};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State, Wry};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};

pub(crate) type R<T> = Result<T, String>;

/// Read-only access to the unlocked store.
fn with_store<T>(state: &State<'_, AppState>, f: impl FnOnce(&Store) -> R<T>) -> R<T> {
    state.with(|inner| f(inner.session.unlocked()?))
}

/// Mutate a copy of the unlocked store, persist it durably, and only
/// then make it live — memory never gets ahead of disk.
///
/// Invariant: MEMORY-FOLLOWS-DISK (ARCHITECTURE.md).
fn mutate<T>(state: &State<'_, AppState>, f: impl FnOnce(&mut Store) -> R<T>) -> R<T> {
    state.with(|inner| {
        let path = inner.store_path.clone();
        let (store, passphrase) = inner.session.unlocked_mut()?;
        let mut next = store.clone();
        let out = f(&mut next)?;
        store::save(&mut next, &path, passphrase.as_deref())
            .map_err(|e| format!("could not save the store: {e}"))?;
        *store = next;
        Ok(out)
    })
}

/// Run a native file dialog on a worker thread and resolve the picked
/// path. `Ok(None)` means the user cancelled.
async fn dialog_path(
    app: &AppHandle,
    open: impl FnOnce(FileDialogBuilder<Wry>) -> Option<FilePath> + Send + 'static,
) -> R<Option<PathBuf>> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || open(dialog.file()))
        .await
        .map_err(|e| format!("dialog failed: {e}"))?;
    picked
        .map(|fp| {
            fp.into_path()
                .map_err(|e| format!("unsupported file location: {e}"))
        })
        .transpose()
}

/// Read a user-picked text file (an .env, an example, a merge target),
/// lossily as UTF-8, refusing anything over 2 MB.
fn read_text_capped(path: &Path) -> R<String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if bytes.len() > 2_000_000 {
        return Err("that file is larger than 2 MB — too big for an .env file".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn latest_effective(project: &Project) -> Vec<(String, String)> {
    project
        .latest()
        .map(|s| envfile::effective_entries(&envfile::parse(&s.raw)))
        .unwrap_or_default()
}

// ------------------------------------------------------------------ misc

#[tauri::command]
pub fn ui_log(level: String, message: String) {
    println!("[ui:{level}] {message}");
}
