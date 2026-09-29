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
use crate::state::AppState;
use crate::store::{self, Project, Store};
use tauri::State;

pub(crate) type R<T> = Result<T, String>;

fn selftest_active() -> bool {
    std::env::var("ENVARSA_SELFTEST").is_ok()
}

/// Read-only access to the unlocked store.
fn with_store<T>(state: &State<'_, AppState>, f: impl FnOnce(&Store) -> R<T>) -> R<T> {
    state.with(|inner| f(inner.session.unlocked()?))
}

/// Mutate a copy of the unlocked store, persist it durably, and only
/// then make it live — memory never gets ahead of disk.
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
