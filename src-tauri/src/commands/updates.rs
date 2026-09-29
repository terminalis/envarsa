use super::{selftest_active, R};
use crate::state::{self, AppState};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

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
pub async fn check_for_updates(app: AppHandle, state: State<'_, AppState>) -> R<UpdateCheckResult> {
    if selftest_active() {
        return Err("update checks are disabled during selftest".into());
    }
    if crate::update::is_packaged() {
        return Err(
            "This is the Microsoft Store build — it updates through the Store, so the in-app check is off."
                .into(),
        );
    }
    let latest = tauri::async_runtime::spawn_blocking(crate::update::fetch_latest_version)
        .await
        .map_err(|e| format!("update check failed: {e}"))??;
    let current = app.package_info().version.clone();
    let newer = latest > current;
    // Best effort: a config-write failure must not eat a good answer.
    // A manual check legitimately postpones the next automatic one.
    let _ = state.with(|inner| {
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
    state.with(|inner| {
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
