use super::{with_store, R};
use crate::clipboard::copy_secret_to_clipboard;
use crate::envfile::{self, Line};
use crate::state::AppState;
use crate::store::Store;
use serde::Serialize;
use tauri::State;

// -------------------------------------------------------- reveal & copy

fn line_at(store: &Store, project_id: &str, snapshot_id: &str, idx: usize) -> R<Line> {
    let (_, snapshot) = store.find(project_id, Some(snapshot_id))?;
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
            Line::Bad { raw } => Ok(RevealedValue {
                key: String::new(),
                value: raw,
            }),
            _ => Err("that line has no value to reveal".into()),
        }
    })
}

/// Copies a single value Rust → OS clipboard; the value never transits
/// the webview. The copy happens after the state is unlocked, since on
/// Linux it waits for the GTK main thread.
#[tauri::command]
pub fn copy_value(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    idx: usize,
) -> R<String> {
    let (key, value) = with_store(&state, |store| {
        match line_at(store, &project_id, &snapshot_id, idx)? {
            Line::Entry { key, value, .. } => Ok((key, value)),
            _ => Err("that line is not an entry".into()),
        }
    })?;
    copy_secret_to_clipboard(value)?;
    Ok(key)
}

/// Copies the whole snapshot block (raw bytes, exactly as captured).
#[tauri::command]
pub fn copy_block(state: State<'_, AppState>, project_id: String, snapshot_id: String) -> R<usize> {
    let raw = with_store(&state, |store| {
        Ok(store.find(&project_id, Some(&snapshot_id))?.1.raw.clone())
    })?;
    let count = envfile::entry_count(&raw);
    copy_secret_to_clipboard(raw)?;
    Ok(count)
}
