use super::{with_store, R};
use crate::clipboard::copy_secret_to_clipboard;
use crate::envfile::{self, Line};
use crate::state::AppState;
use crate::store::Store;
use serde::Serialize;
use tauri::State;

// -------------------------------------------------------- reveal & copy

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
pub fn copy_block(state: State<'_, AppState>, project_id: String, snapshot_id: String) -> R<usize> {
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
