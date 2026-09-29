use super::R;
use crate::state::{self, AppState, Inner, Session};
use crate::store::{self, Opened, Store};
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::State;

// ---------------------------------------------------------------- status

/// Which gate, if any, the UI shows before the library.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Unlocked,
    Locked,
    Corrupt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub state: SessionState,
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

fn status_of(inner: &Inner) -> StatusPayload {
    let (state, encrypted, project_count, error) = match &inner.session {
        Session::Unlocked { store, passphrase } => (
            SessionState::Unlocked,
            passphrase.is_some(),
            Some(store.projects.len()),
            None,
        ),
        Session::Locked => (SessionState::Locked, true, None, None),
        Session::Corrupt { error } => (SessionState::Corrupt, false, None, Some(error.clone())),
    };
    let packaged = crate::update::is_packaged();
    let version = crate::update::running_version();
    StatusPayload {
        state,
        store_path: inner.store_path.to_string_lossy().to_string(),
        encrypted,
        env_override: inner.env_override,
        portable: crate::state::portable_base().is_some(),
        backup_exists: store::backup_path(&inner.store_path).exists(),
        project_count,
        error,
        app_version: version.to_string(),
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
                .filter(|v| *v > version)
                .map(|v| v.to_string())
        },
        auto_update_check: inner.config.auto_update_check,
        packaged,
    }
}

#[tauri::command]
pub fn store_status(state: State<'_, AppState>) -> R<StatusPayload> {
    state.with(|inner| Ok(status_of(inner)))
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
//
// Each command checks its own preconditions, then reprotect saves the
// store under the new protection and realigns the backup.

/// The passphrase rule for at-rest encryption and encrypted exports.
pub(crate) fn check_passphrase(passphrase: &str) -> R<()> {
    if passphrase.chars().count() < 8 {
        return Err("use at least 8 characters".into());
    }
    Ok(())
}

/// The unlocked store and its passphrase, when encryption is on.
fn encrypted(session: &mut Session) -> R<(&mut Store, &mut Option<String>)> {
    match session.unlocked_mut() {
        Ok((store, held)) if held.is_some() => Ok((store, held)),
        _ => Err("encryption is not enabled".into()),
    }
}

/// Save the store under `next` (a passphrase, or None for plaintext) and
/// only then adopt it, so a failed save leaves the session matching the
/// file on disk. The save keeps the previous bytes as `.bak`, which are
/// under the old protection; rewrite it so, for example, no plaintext
/// copy outlives encrypting. `backup_err` says where that leaves things
/// if only the backup rewrite fails.
///
/// Invariants: MEMORY-FOLLOWS-DISK and BACKUP-MATCHES-PROTECTION (ARCHITECTURE.md).
fn reprotect(
    path: &Path,
    store: &mut Store,
    held: &mut Option<String>,
    next: Option<String>,
    backup_err: &str,
) -> R<()> {
    store::save(store, path, next.as_deref())?;
    *held = next;
    store::align_backup(path).map_err(|e| format!("{backup_err} — {e}"))
}

fn enable(inner: &mut Inner, passphrase: String) -> R<()> {
    check_passphrase(&passphrase)?;
    let (store, held) = inner
        .session
        .unlocked_mut()
        .map_err(|_| "unlock the store first".to_string())?;
    if held.is_some() {
        return Err("encryption is already enabled".into());
    }
    reprotect(
        &inner.store_path,
        store,
        held,
        Some(passphrase),
        "the store is encrypted, but the old plaintext backup survived",
    )
}

fn change(inner: &mut Inner, current: String, new_passphrase: String) -> R<()> {
    check_passphrase(&new_passphrase)?;
    let (store, held) = encrypted(&mut inner.session)?;
    if held.as_deref() != Some(current.as_str()) {
        return Err("the current passphrase is not right".into());
    }
    reprotect(
        &inner.store_path,
        store,
        held,
        Some(new_passphrase),
        "the passphrase was changed, but the backup still uses the old one",
    )
}

fn disable(inner: &mut Inner, passphrase: String) -> R<()> {
    let (store, held) = encrypted(&mut inner.session)?;
    if held.as_deref() != Some(passphrase.as_str()) {
        return Err("the passphrase is not right".into());
    }
    // Keep the backup in step with the live store's protection state,
    // so a later restore can't silently re-encrypt (or vice versa).
    reprotect(
        &inner.store_path,
        store,
        held,
        None,
        "the store is decrypted, but the backup could not be rewritten",
    )
}

#[tauri::command]
pub fn enable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    state.with(|inner| enable(inner, passphrase))
}

#[tauri::command]
pub fn change_passphrase(
    state: State<'_, AppState>,
    current: String,
    new_passphrase: String,
) -> R<()> {
    state.with(|inner| change(inner, current, new_passphrase))
}

#[tauri::command]
pub fn disable_encryption(state: State<'_, AppState>, passphrase: String) -> R<()> {
    state.with(|inner| disable(inner, passphrase))
}

/// Restoring is only offered (and only allowed) when the store cannot
/// be loaded — on a healthy session it would be a silent rollback, and
/// on an encrypted one a possible downgrade to whatever the backup
/// holds.
#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>) -> R<StatusPayload> {
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
        Ok(status_of(inner))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::state::Config;
    use crate::store::fixtures::{project, store_of, tmp_dir};
    use std::path::PathBuf;

    const PASS: &str = "correct horse";

    /// An unlocked session over a store saved at `path` (plaintext, or
    /// under `passphrase`).
    fn inner_at(path: PathBuf, passphrase: Option<&str>) -> Inner {
        let mut store = store_of(vec![project("alpha", "A=1\n")]);
        store::save(&mut store, &path, passphrase).unwrap();
        let session = Session::Unlocked {
            store,
            passphrase: passphrase.map(String::from),
        };
        Inner::new(path, PathBuf::new(), Config::default(), false, session)
    }

    fn held(inner: &Inner) -> Option<&str> {
        match &inner.session {
            Session::Unlocked { passphrase, .. } => passphrase.as_deref(),
            _ => panic!("session is not unlocked"),
        }
    }

    fn encrypted_on_disk(path: &Path) -> bool {
        crypto::is_encrypted(&fs::read(path).unwrap())
    }

    #[test]
    fn enable_encrypts_the_store_and_its_backup() {
        let dir = tmp_dir("enable");
        let path = dir.join("envarsa.store");
        let mut inner = inner_at(path.clone(), None);
        // A second save, so a plaintext .bak exists before the transition.
        store::save(inner.session.unlocked_mut().unwrap().0, &path, None).unwrap();

        assert_eq!(
            enable(&mut inner, "short".into()).unwrap_err(),
            "use at least 8 characters"
        );
        enable(&mut inner, PASS.into()).unwrap();
        assert_eq!(held(&inner), Some(PASS));
        assert!(encrypted_on_disk(&path));
        assert!(encrypted_on_disk(&store::backup_path(&path)));
        assert_eq!(
            enable(&mut inner, PASS.into()).unwrap_err(),
            "encryption is already enabled"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn change_and_disable_check_the_held_passphrase() {
        let dir = tmp_dir("change");
        let path = dir.join("envarsa.store");
        let mut inner = inner_at(path.clone(), Some(PASS));

        let wrong = change(&mut inner, "nope".into(), "new passphrase".into());
        assert_eq!(wrong.unwrap_err(), "the current passphrase is not right");
        change(&mut inner, PASS.into(), "new passphrase".into()).unwrap();
        assert_eq!(held(&inner), Some("new passphrase"));
        let bak = fs::read(store::backup_path(&path)).unwrap();
        assert!(
            crypto::decrypt(&bak, "new passphrase").is_ok(),
            "the backup follows the new passphrase"
        );

        assert_eq!(
            disable(&mut inner, PASS.into()).unwrap_err(),
            "the passphrase is not right"
        );
        disable(&mut inner, "new passphrase".into()).unwrap();
        assert_eq!(held(&inner), None);
        assert!(!encrypted_on_disk(&path));
        assert!(!encrypted_on_disk(&store::backup_path(&path)));
        assert_eq!(
            disable(&mut inner, "new passphrase".into()).unwrap_err(),
            "encryption is not enabled"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_failed_save_keeps_the_old_protection() {
        let dir = tmp_dir("reprotect-fail");
        let mut inner = inner_at(dir.join("envarsa.store"), None);
        // A regular file where the store's folder should be: the save
        // cannot create it.
        fs::write(dir.join("blocker"), b"").unwrap();
        inner.store_path = dir.join("blocker").join("envarsa.store");

        assert!(enable(&mut inner, PASS.into()).is_err());
        assert_eq!(held(&inner), None, "memory never gets ahead of disk");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn protection_commands_need_an_unlocked_store() {
        let mut inner = Inner::new(
            PathBuf::new(),
            PathBuf::new(),
            Config::default(),
            false,
            Session::Locked,
        );
        assert_eq!(
            enable(&mut inner, PASS.into()).unwrap_err(),
            "unlock the store first"
        );
        let err = change(&mut inner, PASS.into(), PASS.into()).unwrap_err();
        assert_eq!(err, "encryption is not enabled");
    }
}
