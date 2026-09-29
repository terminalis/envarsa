use super::export::stage_write;
use super::transfer::stage_import;
use super::{selftest_active, R};
use crate::clipboard::{clipboard_retry, write_clipboard};
use crate::state::AppState;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn selftest_enabled() -> bool {
    selftest_active()
}

#[tauri::command]
pub fn selftest_read_clipboard() -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    clipboard_retry("read", || {
        arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .map_err(|e| e.to_string())
    })
}

/// No auto-clear here — the selftest uses this to put the user's
/// original clipboard back when it finishes.
#[tauri::command]
pub fn selftest_set_clipboard(text: String) -> R<()> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    write_clipboard(text)
}

/// Test hook: read a file back (lossy text), e.g. an export or the
/// `.bak` sibling, to assert on the bytes Envarsa wrote. Selftest-only.
#[tauri::command]
pub fn selftest_read_file(path: String) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Test hook: stage an import by path, standing in for the picker
/// dialog (which needs a human). Selftest-only.
#[tauri::command]
pub fn selftest_stage_import(state: State<'_, AppState>, path: String) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    stage_import(&state, PathBuf::from(path))
}

/// Test hooks: stage a `.env.local` write target (and an example
/// template) by path, standing in for the picker dialogs. Only the
/// staging is selftest-gated; the write commands are real features, so
/// the "path never comes from the webview" rule holds in normal runs.
#[tauri::command]
pub fn selftest_stage_write(state: State<'_, AppState>, path: String) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    stage_write(&state, PathBuf::from(path), None)
}

#[tauri::command]
pub fn selftest_stage_example(
    state: State<'_, AppState>,
    out_path: String,
    template: String,
) -> R<String> {
    if !selftest_active() {
        return Err("selftest-only command".into());
    }
    stage_write(&state, PathBuf::from(out_path), Some(template))
}

#[tauri::command]
pub fn selftest_done(app: AppHandle, passed: usize, failed: usize, report: String) {
    if !selftest_active() {
        return; // not an exit lever for normal runs
    }
    use std::io::Write as _;
    println!("SELFTEST REPORT\n{report}\nSELFTEST: {passed} passed, {failed} failed");
    let _ = std::io::stdout().flush();
    app.exit(if failed == 0 { 0 } else { 1 });
}
