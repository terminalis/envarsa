use super::{dialog_path, latest_effective, mutate, with_store, R};
use crate::envfile::{self, Line};
use crate::state::{AppState, Session};
use crate::store::{self, Project, Snapshot, Store};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

// -------------------------------------------------------------- projects

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub path_hint: Option<String>,
    pub snapshot_count: usize,
    pub entry_count: usize,
    pub latest_captured_at: Option<String>,
    /// How many of this project's keys also exist in other projects.
    pub shared_keys: usize,
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> R<Vec<ProjectMeta>> {
    with_store(&state, |store| {
        let all_effective: Vec<(String, Vec<(String, String)>)> = store
            .projects
            .iter()
            .map(|p| (p.id.clone(), latest_effective(p)))
            .collect();

        let mut metas: Vec<ProjectMeta> = store
            .projects
            .iter()
            .map(|p| {
                let mine = latest_effective(p);
                let shared = mine
                    .iter()
                    .filter(|(k, _)| {
                        all_effective.iter().any(|(other_id, entries)| {
                            other_id != &p.id && entries.iter().any(|(ok, _)| ok == k)
                        })
                    })
                    .count();
                ProjectMeta {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    path_hint: p.path_hint.clone(),
                    snapshot_count: p.snapshots.len(),
                    entry_count: mine.len(),
                    latest_captured_at: p.latest().map(|s| s.captured_at.clone()),
                    shared_keys: shared,
                }
            })
            .collect();
        metas.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(metas)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReuseRef {
    pub project_id: String,
    pub name: String,
    pub same: bool,
}

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum LineView {
    Blank,
    Comment {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Entry {
        idx: usize,
        key: String,
        exported: bool,
        /// True when a later line in the same snapshot overrides this key.
        overridden: bool,
        reuse: Vec<ReuseRef>,
    },
    /// The raw text stays out of the listing — a malformed line is as
    /// likely as any to hold a secret, so it is masked like a value and
    /// crosses only through `reveal_value`.
    Bad {
        idx: usize,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub captured_at: String,
    pub via: String,
    pub source_path: Option<String>,
    pub entry_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub path_hint: Option<String>,
    pub created_at: String,
    /// Newest first.
    pub snapshots: Vec<SnapshotMeta>,
    pub snapshot_id: String,
    pub captured_at: String,
    pub via: String,
    pub source_path: Option<String>,
    pub is_latest: bool,
    pub entry_count: usize,
    pub lines: Vec<LineView>,
}

#[tauri::command]
pub fn get_project(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: Option<String>,
) -> R<ProjectView> {
    with_store(&state, |store| {
        let (project, snapshot) = store.find(&project_id, snapshot_id.as_deref())?;
        let is_latest = project
            .latest()
            .map(|s| s.id == snapshot.id)
            .unwrap_or(false);

        // Other projects' current entries, for reuse flags.
        let others: Vec<(&Project, Vec<(String, String)>)> = store
            .projects
            .iter()
            .filter(|p| p.id != project.id)
            .map(|p| (p, latest_effective(p)))
            .collect();

        let lines = envfile::parse(&snapshot.raw);

        // Which keys are overridden by a later line in this snapshot?
        let mut last_idx: std::collections::HashMap<&str, usize> = Default::default();
        for (i, line) in lines.iter().enumerate() {
            if let Line::Entry { key, .. } = line {
                last_idx.insert(key.as_str(), i);
            }
        }

        let line_views = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| match line {
                Line::Blank => LineView::Blank,
                Line::Comment(text) => LineView::Comment { text: text.clone() },
                Line::Bad(_) => LineView::Bad { idx },
                Line::Entry {
                    key,
                    value,
                    exported,
                } => {
                    let reuse = others
                        .iter()
                        .filter_map(|(p, entries)| {
                            entries
                                .iter()
                                .find(|(k, _)| k == key)
                                .map(|(_, v)| ReuseRef {
                                    project_id: p.id.clone(),
                                    name: p.name.clone(),
                                    same: v == value,
                                })
                        })
                        .collect();
                    LineView::Entry {
                        idx,
                        key: key.clone(),
                        exported: *exported,
                        overridden: last_idx.get(key.as_str()) != Some(&idx),
                        reuse,
                    }
                }
            })
            .collect();

        Ok(ProjectView {
            id: project.id.clone(),
            name: project.name.clone(),
            path_hint: project.path_hint.clone(),
            created_at: project.created_at.clone(),
            snapshots: project
                .snapshots
                .iter()
                .rev()
                .map(|s| SnapshotMeta {
                    id: s.id.clone(),
                    captured_at: s.captured_at.clone(),
                    via: s.via.clone(),
                    source_path: s.source_path.clone(),
                    entry_count: envfile::entry_count(&s.raw),
                })
                .collect(),
            snapshot_id: snapshot.id.clone(),
            captured_at: snapshot.captured_at.clone(),
            via: snapshot.via.clone(),
            source_path: snapshot.source_path.clone(),
            is_latest,
            entry_count: envfile::entry_count(&snapshot.raw),
            lines: line_views,
        })
    })
}

// --------------------------------------------------------------- capture

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePreview {
    pub entries: usize,
    pub comments: usize,
    pub bad: usize,
    pub keys: Vec<String>,
    pub dup_keys: Vec<String>,
}

#[tauri::command]
pub fn preview_capture(text: String) -> R<CapturePreview> {
    let lines = envfile::parse(&text);
    let mut keys = Vec::new();
    let mut dups = Vec::new();
    let mut comments = 0;
    let mut bad = 0;
    for line in &lines {
        match line {
            Line::Comment(_) => comments += 1,
            Line::Bad(_) => bad += 1,
            Line::Entry { key, .. } => {
                if keys.contains(key) {
                    if !dups.contains(key) {
                        dups.push(key.clone());
                    }
                } else {
                    keys.push(key.clone());
                }
            }
            Line::Blank => {}
        }
    }
    Ok(CapturePreview {
        entries: keys.len(),
        comments,
        bad,
        keys,
        dup_keys: dups,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub path: String,
    pub dir: Option<String>,
    pub name_guess: Option<String>,
    pub text: String,
}

fn read_env_file(path: &Path) -> R<PickedFile> {
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if bytes.len() > 2_000_000 {
        return Err("that file is larger than 2 MB — not an .env file?".into());
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let dir = path.parent().map(|p| p.to_string_lossy().to_string());
    // A .env usually lives in the project root, so the parent folder
    // name is a good default project name.
    let name_guess = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    Ok(PickedFile {
        path: path.to_string_lossy().to_string(),
        dir,
        name_guess,
        text,
    })
}

#[tauri::command]
pub async fn pick_env_file(app: AppHandle) -> R<Option<PickedFile>> {
    dialog_path(&app, |d| {
        d.set_title("Choose a .env file to capture")
            .blocking_pick_file()
    })
    .await?
    .map(|path| read_env_file(&path))
    .transpose()
}

/// Drag/drop is handled as a window event so the path never round-trips
/// through the webview: Rust reads the file and hands the UI a finished
/// payload. There is no path-taking IPC command for webview JavaScript
/// to call.
pub fn handle_drop(window: &tauri::Window, paths: &[PathBuf]) {
    let state = window.state::<AppState>();
    let unlocked = state
        .with(|inner| Ok(matches!(inner.session, Session::Unlocked { .. })))
        .unwrap_or(false);
    if !unlocked {
        return;
    }
    let Some(path) = paths.first() else { return };
    match read_env_file(path) {
        Ok(picked) => {
            let _ = window.emit("env-file-dropped", &picked);
        }
        Err(e) => {
            let _ = window.emit("env-drop-error", &e);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureArgs {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub path_hint: Option<String>,
    pub text: String,
    pub source_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub project_id: String,
    pub snapshot_id: String,
    pub entry_count: usize,
}

/// Push `snapshot` onto a project, applying an optional path-hint
/// update. Shared by capture and the structured editor: the project is
/// found by `project_id`, else by `project_name`, else created under
/// that name.
fn add_snapshot(
    store: &mut Store,
    project_id: Option<&str>,
    project_name: Option<&str>,
    path_hint: Option<&str>,
    snapshot: Snapshot,
) -> R<CaptureResult> {
    let project_id = match project_id {
        Some(id) => store.find_mut(id)?.id.clone(),
        None => {
            let name = project_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "give the project a name".to_string())?;
            match store.project_by_name(name) {
                Some(existing) => existing.id.clone(),
                None => {
                    let project = Project {
                        id: store::new_id(),
                        name: name.to_string(),
                        path_hint: None,
                        created_at: store::now_iso(),
                        snapshots: Vec::new(),
                    };
                    let id = project.id.clone();
                    store.projects.push(project);
                    id
                }
            }
        }
    };
    let result = CaptureResult {
        project_id: project_id.clone(),
        snapshot_id: snapshot.id.clone(),
        entry_count: envfile::entry_count(&snapshot.raw),
    };
    let p = store.find_mut(&project_id)?;
    if let Some(hint) = path_hint.map(str::trim).filter(|s| !s.is_empty()) {
        p.path_hint = Some(hint.to_string());
    }
    p.snapshots.push(snapshot);
    Ok(result)
}

#[tauri::command]
pub fn capture(state: State<'_, AppState>, args: CaptureArgs) -> R<CaptureResult> {
    let via = if args.source_path.is_some() {
        "file"
    } else {
        "paste"
    };
    let snapshot = Snapshot::new(via, args.source_path, args.text);
    mutate(&state, |store| {
        add_snapshot(
            store,
            args.project_id.as_deref(),
            args.project_name.as_deref(),
            args.path_hint.as_deref(),
            snapshot,
        )
    })
}
// ----------------------------------------------------- structured editor
//
// The editor builds a draft line list in the webview, then saves it as a
// new snapshot through the normal capture path — so the store keeps raw
// text verbatim and history is preserved. This is also the store-only
// "new project by hand" path: no file in, no file out.

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditLine {
    Blank,
    Comment {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Entry {
        key: String,
        value: String,
        exported: bool,
    },
    Bad {
        raw: String,
    },
}

fn edit_line_to_line(e: &EditLine) -> Line {
    match e {
        EditLine::Blank => Line::Blank,
        EditLine::Comment { text } => {
            let t = text.replace(['\n', '\r'], " ");
            let t = t.trim_end();
            if t.trim_start().starts_with('#') {
                Line::Comment(t.to_string())
            } else {
                Line::Comment(format!("# {}", t.trim_start()))
            }
        }
        EditLine::Entry {
            key,
            value,
            exported,
        } => Line::Entry {
            key: key.trim().to_string(),
            value: value.clone(),
            exported: *exported,
        },
        EditLine::Bad { raw } => Line::Bad(raw.clone()),
    }
}

/// Seed the editor from an existing snapshot (latest if none given).
/// Values cross here — the editor needs them; gated on an unlocked store
/// and an explicit user action.
#[tauri::command]
pub fn edit_lines(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: Option<String>,
) -> R<Vec<EditLine>> {
    with_store(&state, |store| {
        let (_, snapshot) = store.find(&project_id, snapshot_id.as_deref())?;
        Ok(envfile::parse(&snapshot.raw)
            .into_iter()
            .map(|l| match l {
                Line::Blank => EditLine::Blank,
                Line::Comment(text) => EditLine::Comment { text },
                Line::Entry {
                    key,
                    value,
                    exported,
                } => EditLine::Entry {
                    key,
                    value,
                    exported,
                },
                Line::Bad(raw) => EditLine::Bad { raw },
            })
            .collect())
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEditArgs {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub path_hint: Option<String>,
    pub lines: Vec<EditLine>,
}

/// Save the edited lines as a new snapshot (`via: "edit"`). Resolves or
/// creates the project like capture, so this is the store-only new-project
/// path too. Validates keys before mutating.
#[tauri::command]
pub fn save_edited_snapshot(state: State<'_, AppState>, args: SaveEditArgs) -> R<CaptureResult> {
    for line in &args.lines {
        if let EditLine::Entry { key, .. } = line {
            let k = key.trim();
            let bad = k.is_empty()
                || k.chars()
                    .any(|c| c.is_whitespace() || c == '#' || c == '"' || c == '\'');
            if bad {
                return Err(format!(
                    "\"{key}\" is not a valid key — keys can't be empty or contain spaces, #, \", or '"
                ));
            }
        }
    }
    let lines: Vec<Line> = args.lines.iter().map(edit_line_to_line).collect();
    let raw = envfile::serialize_lines(&lines);

    let snapshot = Snapshot::new("edit", None, raw);
    mutate(&state, |store| {
        add_snapshot(
            store,
            args.project_id.as_deref(),
            args.project_name.as_deref(),
            args.path_hint.as_deref(),
            snapshot,
        )
    })
}

// ----------------------------------------------------- project editing

#[tauri::command]
pub fn rename_project(state: State<'_, AppState>, project_id: String, name: String) -> R<()> {
    mutate(&state, |store| {
        let name = name.trim();
        if name.is_empty() {
            return Err("the name cannot be empty".into());
        }
        if let Some(other) = store.project_by_name(name) {
            if other.id != project_id {
                return Err(format!("a project named \"{}\" already exists", other.name));
            }
        }
        let p = store.find_mut(&project_id)?;
        p.name = name.to_string();
        Ok(())
    })
}

#[tauri::command]
pub fn set_path_hint(state: State<'_, AppState>, project_id: String, path_hint: String) -> R<()> {
    mutate(&state, |store| {
        let p = store.find_mut(&project_id)?;
        let trimmed = path_hint.trim();
        p.path_hint = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        Ok(())
    })
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, project_id: String) -> R<()> {
    mutate(&state, |store| {
        let before = store.projects.len();
        store.projects.retain(|p| p.id != project_id);
        if store.projects.len() == before {
            return Err("project not found".into());
        }
        Ok(())
    })
}

/// Re-adds an older snapshot as the newest one ("bring this back").
#[tauri::command]
pub fn promote_snapshot(
    state: State<'_, AppState>,
    project_id: String,
    snapshot_id: String,
) -> R<String> {
    mutate(&state, |store| {
        let p = store.find_mut(&project_id)?;
        let src = p
            .snapshot(&snapshot_id)
            .ok_or_else(|| "snapshot not found".to_string())?;
        let snapshot = Snapshot::new("restore", src.source_path.clone(), src.raw.clone());
        let id = snapshot.id.clone();
        p.snapshots.push(snapshot);
        Ok(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::{project, store_of};

    fn paste(raw: &str) -> Snapshot {
        Snapshot::new("paste", None, raw.into())
    }

    #[test]
    fn add_snapshot_finds_by_id_then_by_name() {
        let mut store = store_of(vec![project("alpha", "A=1\n")]);
        let id = store.projects[0].id.clone();

        let r = add_snapshot(&mut store, Some(&id), None, None, paste("A=2\nB=3\n")).unwrap();
        assert_eq!(r.project_id, id);
        assert_eq!(r.entry_count, 2);
        assert_eq!(store.projects[0].latest().unwrap().id, r.snapshot_id);

        // By name: trimmed and case-insensitive, like every name match.
        let r = add_snapshot(&mut store, None, Some("  ALPHA "), None, paste("A=4\n")).unwrap();
        assert_eq!(r.project_id, id);
        assert_eq!(store.projects.len(), 1);
        assert_eq!(store.projects[0].snapshots.len(), 3);
    }

    #[test]
    fn add_snapshot_creates_a_project_under_a_new_name() {
        let mut store = store_of(vec![project("alpha", "A=1\n")]);
        let r = add_snapshot(
            &mut store,
            None,
            Some(" beta "),
            Some(" /work/beta "),
            paste("X=1\n"),
        )
        .unwrap();
        let beta = store.project(&r.project_id).unwrap();
        assert_eq!(beta.name, "beta");
        assert_eq!(beta.path_hint.as_deref(), Some("/work/beta"));
        assert_eq!(beta.snapshots.len(), 1);
    }

    #[test]
    fn add_snapshot_updates_the_hint_only_when_one_is_given() {
        let mut store = store_of(vec![project("alpha", "A=1\n")]);
        let id = store.projects[0].id.clone();
        add_snapshot(&mut store, Some(&id), None, Some("/a"), paste("A=2\n")).unwrap();
        add_snapshot(&mut store, Some(&id), None, None, paste("A=3\n")).unwrap();
        add_snapshot(&mut store, Some(&id), None, Some("   "), paste("A=4\n")).unwrap();
        assert_eq!(store.projects[0].path_hint.as_deref(), Some("/a"));
    }

    #[test]
    fn add_snapshot_refuses_an_unknown_id_or_a_blank_name() {
        let mut store = store_of(vec![project("alpha", "A=1\n")]);
        let err = add_snapshot(&mut store, Some("nope"), None, None, paste("A=1\n"));
        assert_eq!(err.err().unwrap(), "project not found");
        let err = add_snapshot(&mut store, None, Some("  "), None, paste("A=1\n"));
        assert_eq!(err.err().unwrap(), "give the project a name");
        let err = add_snapshot(&mut store, None, None, None, paste("A=1\n"));
        assert_eq!(err.err().unwrap(), "give the project a name");
        assert_eq!(store.projects[0].snapshots.len(), 1, "nothing was pushed");
    }
}
