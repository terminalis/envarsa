use super::R;
use crate::state::{self, AppState, Inner, Session};
use crate::store::{self, Opened};
use serde::Serialize;
use std::fs;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------- status

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub state: String, // "unlocked" | "locked" | "corrupt"
    pub store_path: String,
    pub encrypted: bool,
    pub env_override: bool,
    /// True for the portable build (an `envarsa.portable` marker sits beside
    /// the exe): the store and config default into that folder, so they
    /// travel with it. Lets the UI warn that relocating the store *outside*
    /// the folder un-anchors it from the portable bundle.
    pub portable: bool,
    pub backup_exists: bool,
    pub project_count: Option<usize>,
    pub error: Option<String>,
    pub app_version: String,
    /// A newer released version, when the last (manual or automatic)
    /// check found one. Re-validated here so a stale or garbage
    /// persisted value can never badge the UI.
    pub update_available: Option<String>,
    pub auto_update_check: bool,
    /// True for the packaged (MSIX / Microsoft Store) build, where updates
    /// come through the Store. The UI hides its update controls then, and
    /// `update_available` is forced to None.
    pub packaged: bool,
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
    let packaged = crate::update::is_packaged();
    StatusPayload {
        state: state_str.to_string(),
        store_path: inner.store_path.to_string_lossy().to_string(),
        encrypted,
        env_override: inner.env_override,
        portable: crate::state::portable_base().is_some(),
        backup_exists: store::backup_path(&inner.store_path).exists(),
        project_count,
        error,
        app_version: app.package_info().version.to_string(),
        // The Store build never runs the GitHub check, so it must never
        // badge an "available" version either.
        update_available: if packaged {
            None
        } else {
            inner
                .config
                .available_version
                .as_deref()
                .and_then(|t| crate::update::parse_tag(t).ok())
                .filter(|v| *v > app.package_info().version)
                .map(|v| v.to_string())
        },
        auto_update_check: inner.config.auto_update_check,
        packaged,
    }
}

#[tauri::command]
pub fn store_status(app: AppHandle, state: State<'_, AppState>) -> R<StatusPayload> {
    state.with(|inner| Ok(status_of(&app, inner)))
}

#[tauri::command]
pub fn unlock(state: State<'_, AppState>, passphrase: String) -> R<()> {
    state.with(|inner| {
        let bytes =
            fs::read(&inner.store_path).map_err(|e| format!("could not read store file: {e}"))?;
        // A plaintext file loads as is (e.g. encryption was disabled
        // elsewhere); the passphrase is kept only for an encrypted one.
        let (store, passphrase) = match store::open(&bytes, Some(&passphrase))? {
            Opened::Encrypted(s) => (s, Some(passphrase)),
            Opened::Plain(s) => (s, None),
            Opened::NeedsPassphrase => return Err("enter the passphrase".into()),
        };
        inner.session = Session::Unlocked { store, passphrase };
        Ok(())
    })
}

#[tauri::command]
pub fn lock(state: State<'_, AppState>) -> R<()> {
    state.with(|inner| match &inner.session {
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

// ------------------------------------------------------------ protection

#[tauri::command]
pub fn enable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    if passphrase.chars().count() < 8 {
        return Err("use at least 8 characters".into());
    }
    state.with(|inner| {
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
    state.with(|inner| {
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
                    format!(
                        "the passphrase was changed, but the backup still uses the old one — {e}"
                    )
                })
            }
            _ => Err("encryption is not enabled".into()),
        }
    })
}

#[tauri::command]
pub fn disable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    state.with(|inner| {
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

/// Restoring is only offered (and only allowed) when the store cannot
/// be loaded — on a healthy session it would be a silent rollback, and
/// on an encrypted one a possible downgrade to whatever the backup
/// holds.
#[tauri::command]
pub fn restore_backup(app: AppHandle, state: State<'_, AppState>) -> R<StatusPayload> {
    state.with(|inner| {
        if !matches!(inner.session, Session::Corrupt { .. }) {
            return Err(
                "the store loaded fine — restoring the backup is only for when it cannot be read"
                    .into(),
            );
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
