use super::session::check_passphrase;
use super::{dialog_path, read_text_capped, with_store, R};
use crate::crypto;
use crate::envfile::{self, merge_with_report, AbsentPolicy, MergeReport};
use crate::envpath::classify_name;
use crate::state::AppState;
use crate::store;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

// ---------------------------------------------------------------- export

/// Export = save dialog; the user places the file themselves. Envarsa
/// never writes into a project tree on its own.
#[tauri::command]
pub async fn export_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<Option<String>> {
    let (raw, suggested) = snapshot_export(&state, &project_id, &snapshot_id)?;

    let Some(path) = dialog_path(&app, move |d| {
        d.set_title("Export snapshot as .env")
            .set_file_name(&suggested)
            .blocking_save_file()
    })
    .await?
    else {
        return Ok(None);
    };
    fs::write(&path, raw.as_bytes())
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

/// A snapshot's exact bytes, and the file name to suggest for them.
pub(super) fn snapshot_export(
    state: &State<'_, AppState>,
    project_id: &str,
    snapshot_id: &str,
) -> R<(String, String)> {
    with_store(state, |store| {
        let (project, snapshot) = store.find(project_id, Some(snapshot_id))?;
        Ok((snapshot.raw.clone(), format!("{}.env", project.name)))
    })
}

// ----------------------------------------------------- store export

/// Serialize the live store, optionally encrypting the copy with a
/// transport passphrase (independent of the at-rest one).
pub(super) fn store_copy_bytes(
    state: &State<'_, AppState>,
    passphrase: Option<&str>,
) -> R<Vec<u8>> {
    if let Some(p) = passphrase {
        check_passphrase(p)?;
    }
    let bytes = with_store(state, |store| Ok(store::serialize_store(store)))?;
    match passphrase {
        Some(p) => crypto::encrypt(&bytes, p),
        None => Ok(bytes),
    }
}

/// Save a copy of the whole library wherever the user chooses — the
/// "hand it to another machine" path, without digging through AppData.
/// The live store and its at-rest encryption are untouched.
#[tauri::command]
pub async fn export_store(
    app: AppHandle,
    state: State<'_, AppState>,
    passphrase: Option<String>,
) -> R<Option<String>> {
    let bytes = store_copy_bytes(&state, passphrase.as_deref())?;
    let suggested = format!("envarsa-{}.store", chrono::Local::now().format("%Y-%m-%d"));

    let Some(path) = dialog_path(&app, move |d| {
        d.set_title("Export a copy of the store")
            .set_file_name(&suggested)
            .add_filter("Envarsa store", &["store"])
            .blocking_save_file()
    })
    .await?
    else {
        return Ok(None);
    };
    fs::write(&path, &bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

// ------------------------------------------------------ write .env.local
//
// The one place Envarsa writes into a project tree. The target is always
// a `.env*.local` (gitignored), never an example file (git-committed —
// secrets would leak). The guard is enforced here on the final path; the
// destination is staged behind an opaque token, so the webview never
// supplies a path. The store is never touched — this is an export.

/// The last check before any bytes are written. Run on the final resolved
/// path, regardless of how it was chosen.
fn guard_writable_local(path: &Path) -> R<()> {
    match classify_name(path).refusal() {
        Some(why) => Err(why.into()),
        None => Ok(()),
    }
}

fn stage_write(state: &State<'_, AppState>, path: PathBuf, template: Option<String>) -> R<String> {
    state.with(|inner| Ok(inner.stage_write(path, template)))
}

fn clear_pending_write(state: &State<'_, AppState>) {
    let _ = state.with(|inner| {
        inner.clear_pending_writes();
        Ok(())
    });
}

/// How a staged write fills its target. The staged kind decides: a
/// write staged with an example template fills the example.
enum Fill {
    /// The write dialog's target tab: the snapshot's own lines, merged
    /// into an existing target when `merge` is set.
    Snapshot { merge: bool },
    /// The example tab: the snapshot's values poured into an example's
    /// comments and keys; keys the snapshot lacks are blanked.
    Example { template: String },
}

/// Look up a staged write by token. `merge` applies to a plain target;
/// an example scaffold always fills its template.
fn staged(state: &State<'_, AppState>, token: &str, merge: bool) -> R<(PathBuf, Fill)> {
    state.with(|inner| {
        let p = inner
            .pending_write(token)
            .ok_or("that write is no longer staged — choose the location again")?;
        let fill = match &p.template {
            Some(template) => Fill::Example {
                template: template.clone(),
            },
            None => Fill::Snapshot { merge },
        };
        Ok((p.path.clone(), fill))
    })
}

/// The text a write puts at `path`, and its preview. Preview and write
/// both come from here, so what the user approves is what gets written.
fn plan_write(path: &Path, fill: &Fill, raw: &str) -> R<(String, WritePreview)> {
    let snap_lines = envfile::parse(raw);
    let source = envfile::effective_entries(&snap_lines);
    let (text, report) = match fill {
        Fill::Example { template } => {
            let target = envfile::parse(template);
            merge_with_report(&target, &source, AbsentPolicy::EmptyOut)
        }
        Fill::Snapshot { merge: true } if path.exists() => {
            let target = envfile::parse(&read_text_capped(path)?);
            merge_with_report(&target, &source, AbsentPolicy::KeepTarget)
        }
        // A new file, or a replace: the snapshot's own re-serialized
        // lines. Nothing is merged, so there is nothing to report beyond
        // the entry count.
        Fill::Snapshot { .. } => (
            envfile::serialize_lines(&snap_lines),
            MergeReport::default(),
        ),
    };
    let preview = WritePreview {
        result_entry_count: envfile::entry_count(&text),
        added: report.added,
        substituted: report.substituted,
        emptied: report.emptied,
        kept: report.kept,
        blocked: None,
    };
    Ok((text, preview))
}

fn snapshot_raw(state: &State<'_, AppState>, project_id: &str, snapshot_id: &str) -> R<String> {
    with_store(state, |store| {
        Ok(store.find(project_id, Some(snapshot_id))?.1.raw.clone())
    })
}

/// Preview a staged write. A guard refusal comes back as `blocked`, so
/// the UI can show it without throwing.
fn preview(
    state: &State<'_, AppState>,
    project_id: &str,
    snapshot_id: &str,
    token: &str,
    merge: bool,
) -> R<WritePreview> {
    let (path, fill) = staged(state, token, merge)?;
    let blocked = classify_name(&path).refusal().map(String::from);
    let raw = snapshot_raw(state, project_id, snapshot_id)?;
    let (_, preview) = plan_write(&path, &fill, &raw)?;
    Ok(WritePreview { blocked, ..preview })
}

/// Carry out a staged write. A finished write closes the dialog, so it
/// clears every staged write.
fn write(
    state: &State<'_, AppState>,
    project_id: &str,
    snapshot_id: &str,
    token: &str,
    merge: bool,
) -> R<String> {
    let (path, fill) = staged(state, token, merge)?;
    guard_writable_local(&path)?;
    let raw = snapshot_raw(state, project_id, snapshot_id)?;
    let (text, _) = plan_write(&path, &fill, &raw)?;
    store::write_atomic(&path, text.as_bytes())?;
    clear_pending_write(state);
    Ok(path.to_string_lossy().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTarget {
    /// Opaque handle; the write must present it back. Path stays Rust-side.
    pub token: String,
    /// Display only — the webview never sends a path back.
    pub path: String,
    /// The target's directory, to seed a "change location" dialog.
    pub dir: String,
    pub class: String,
    pub exists: bool,
}

/// Stage `path` as the target tab's write and describe it.
pub(super) fn stage_target(state: &State<'_, AppState>, path: PathBuf) -> R<WriteTarget> {
    Ok(WriteTarget {
        class: classify_name(&path).as_str().to_string(),
        exists: path.exists(),
        dir: path
            .parent()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: path.to_string_lossy().to_string(),
        token: stage_write(state, path, None)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePreview {
    pub result_entry_count: usize,
    pub added: Vec<String>,
    pub substituted: Vec<String>,
    pub emptied: Vec<String>,
    pub kept: Vec<String>,
    /// A guard refusal, so the UI can show it without throwing.
    pub blocked: Option<String>,
}

/// Stage the default target: `<remembered dir>/.env.local`. The dir is
/// the folder of the file the snapshot was captured from (recorded by
/// the core when that file was picked or dropped), else the project
/// folder the user typed. Either way the dialog shows the path before
/// anything is written, and the name guard still applies.
#[tauri::command]
pub fn stage_write_target(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<WriteTarget> {
    let dir = with_store(&state, |store| {
        let (project, snapshot) = store.find(&project_id, Some(&snapshot_id))?;
        let from_source = snapshot
            .source_path
            .as_deref()
            .map(PathBuf::from)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let from_hint = project.path_hint.as_deref().map(PathBuf::from);
        from_source.or(from_hint).ok_or_else(|| {
            "no remembered directory for this project — use “Change location” to choose where to write"
                .to_string()
        })
    })?;
    stage_target(&state, dir.join(".env.local"))
}

/// Redirect the target via a save dialog. `suggested_dir` only seeds the
/// dialog's starting folder; the staged path is the user's actual pick.
#[tauri::command]
pub async fn pick_write_target(
    app: AppHandle,
    state: State<'_, AppState>,
    suggested_dir: Option<String>,
) -> R<Option<WriteTarget>> {
    dialog_path(&app, move |d| {
        let mut d = d.set_title("Write .env.local").set_file_name(".env.local");
        if let Some(dir) = suggested_dir.as_deref() {
            d = d.set_directory(dir);
        }
        d.blocking_save_file()
    })
    .await?
    .map(|path| stage_target(&state, path))
    .transpose()
}

/// Preview either tab's staged write; the token's staged kind decides
/// whether it is a target or an example scaffold.
#[tauri::command]
pub fn preview_write(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
    merge: bool,
) -> R<WritePreview> {
    preview(&state, &project_id, &snapshot_id, &token, merge)
}

#[tauri::command]
pub fn write_env_local(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
    token: String,
    merge: bool,
) -> R<String> {
    write(&state, &project_id, &snapshot_id, &token, merge)
}

// --- import a .env.example as a scaffold, write .env.local beside it ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleStaged {
    pub token: String,
    pub example_name: String,
    /// Display only — the staged output path (`<example dir>/.env.local`).
    pub out_path: String,
    pub out_class: String,
    pub example_keys: Vec<String>,
}

/// Pick a `.env.example` to use as a template. Only its text is read; the
/// staged write target is `<example dir>/.env.local` — the example path
/// is never staged for writing.
#[tauri::command]
pub async fn pick_example_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> R<Option<ExampleStaged>> {
    dialog_path(&app, |d| {
        d.set_title("Choose a .env.example to use as a template")
            .blocking_pick_file()
    })
    .await?
    .map(|path| stage_example(&state, &path))
    .transpose()
}

/// Read the example at `path` and stage `<its dir>/.env.local` as the
/// write it scaffolds.
pub(super) fn stage_example(state: &State<'_, AppState>, path: &Path) -> R<ExampleStaged> {
    let template = read_text_capped(path)?;
    let example_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let out_path = path
        .parent()
        .map(|d| d.join(".env.local"))
        .ok_or_else(|| "that file has no parent directory".to_string())?;
    let example_keys: Vec<String> = envfile::effective_entries(&envfile::parse(&template))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    Ok(ExampleStaged {
        out_class: classify_name(&out_path).as_str().to_string(),
        out_path: out_path.to_string_lossy().to_string(),
        token: stage_write(state, out_path, Some(template))?,
        example_name,
        example_keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::tmp_dir;

    const RAW: &str = "# snap\nPORT=3000\nAPI_KEY=secret\n";

    const MERGE: Fill = Fill::Snapshot { merge: true };
    const REPLACE: Fill = Fill::Snapshot { merge: false };

    #[test]
    fn a_missing_target_is_written_fresh_even_when_merging() {
        let dir = tmp_dir("plan-fresh");
        let path = dir.join(".env.local");
        let (text, p) = plan_write(&path, &MERGE, RAW).unwrap();
        assert_eq!(text, RAW);
        assert!(p.added.is_empty() && p.kept.is_empty() && p.substituted.is_empty());
        assert_eq!(p.result_entry_count, 2);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn merge_keeps_target_only_keys_and_overwrite_replaces_them() {
        let dir = tmp_dir("plan-merge");
        let path = dir.join(".env.local");
        fs::write(&path, "# local\nLOCAL_ONLY=keep\nPORT=old\n").unwrap();

        let (text, p) = plan_write(&path, &MERGE, RAW).unwrap();
        assert_eq!(
            text,
            "# local\nLOCAL_ONLY=keep\nPORT=3000\n\n# Added by Envarsa\nAPI_KEY=secret\n"
        );
        assert_eq!(p.added, ["API_KEY"]);
        assert_eq!(p.substituted, ["PORT"]);
        assert_eq!(p.kept, ["LOCAL_ONLY"]);
        assert_eq!(p.result_entry_count, 3);

        let (text, p) = plan_write(&path, &REPLACE, RAW).unwrap();
        assert_eq!(text, RAW);
        assert!(p.kept.is_empty() && p.substituted.is_empty());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn an_example_is_filled_and_its_unmatched_keys_blanked() {
        let fill = Fill::Example {
            template: "# API\nAPI_KEY=put-your-key-here\nUNUSED=placeholder\n".into(),
        };
        // The example fill ignores whatever is at the target path.
        let (text, p) = plan_write(Path::new("/nowhere/.env.local"), &fill, RAW).unwrap();
        assert_eq!(
            text,
            "# API\nAPI_KEY=secret\nUNUSED=\n\n# Added by Envarsa\nPORT=3000\n"
        );
        assert_eq!(p.substituted, ["API_KEY"]);
        assert_eq!(p.emptied, ["UNUSED"]);
        assert_eq!(p.added, ["PORT"]);
        assert!(p.blocked.is_none());
    }

    #[test]
    fn only_the_local_family_is_writable() {
        assert!(guard_writable_local(Path::new("/p/.env.local")).is_ok());
        assert!(guard_writable_local(Path::new("/p/.env.development.local")).is_ok());
        let example = guard_writable_local(Path::new("/p/.env.example")).unwrap_err();
        assert!(example.contains("example file"), "{example}");
        assert!(guard_writable_local(Path::new("/p/notes.txt")).is_err());
    }
}
