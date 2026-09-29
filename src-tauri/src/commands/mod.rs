//! The IPC surface. The webview can only reach these commands; every
//! OS interaction (file dialogs, clipboard, shell reveal) stays in the
//! Rust core. Values cross into the webview only on an explicit
//! per-value reveal — listing a project sends keys and structure, never
//! values. "Copy" hands a value straight from the core to the OS
//! clipboard without it ever transiting the UI.

pub(crate) mod export;
pub(crate) mod library;
pub(crate) mod secrets;
pub(crate) mod selftest;
pub(crate) mod session;
pub(crate) mod transfer;
pub(crate) mod updates;

use crate::envfile;
use crate::state::{AppState, Inner, Session};
use crate::store::{self, Project, Store};
use tauri::State;

pub(crate) type R<T> = Result<T, String>;

fn selftest_active() -> bool {
    std::env::var("ENVARSA_SELFTEST").is_ok()
}

fn with_inner<T>(state: &State<'_, AppState>, f: impl FnOnce(&mut Inner) -> R<T>) -> R<T> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "internal: state poisoned".to_string())?;
    let inner = guard
        .as_mut()
        .ok_or_else(|| "app is still starting".to_string())?;
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
            Session::Corrupt { error } => Err(format!("the store could not be loaded: {error}")),
        }
    })
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
