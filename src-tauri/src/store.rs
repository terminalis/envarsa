//! The store: one portable file, the durable source of truth.
//!
//! Plaintext, pretty-printed JSON by default — the user owns and can
//! inspect the bytes. Optionally age-encrypted with a passphrase
//! (see `crypto`). Saves are atomic (temp file + fsync + rename) and
//! keep a one-step `.bak` sibling of the previous version.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "envarsa-store";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    pub format: String,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    /// The project's identity is this user-chosen name — never a path.
    pub name: String,
    /// Non-binding hint about where the project lives on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_hint: Option<String>,
    pub created_at: String,
    /// Point-in-time captures, oldest first. The last one is current.
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub captured_at: String,
    /// "file" | "paste" | "restore"
    pub via: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// The captured .env text, byte-for-byte.
    pub raw: String,
}

impl Store {
    pub fn new_empty() -> Self {
        let now = now_iso();
        Store {
            format: FORMAT.to_string(),
            version: VERSION,
            created_at: now.clone(),
            updated_at: now,
            projects: Vec::new(),
        }
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn project_mut(&mut self, id: &str) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    pub fn project_by_name(&self, name: &str) -> Option<&Project> {
        let needle = name.trim().to_lowercase();
        self.projects
            .iter()
            .find(|p| p.name.trim().to_lowercase() == needle)
    }
}

impl Project {
    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    pub fn snapshot(&self, id: &str) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn parse_store(bytes: &[u8]) -> Result<Store, String> {
    let store: Store = serde_json::from_slice(bytes)
        .map_err(|e| format!("store file is not valid JSON: {e}"))?;
    if store.format != FORMAT {
        return Err(format!("not an Envarsa store (format: \"{}\")", store.format));
    }
    if store.version > VERSION {
        return Err(format!(
            "this store was written by a newer Envarsa (store version {}, app supports {})",
            store.version, VERSION
        ));
    }
    Ok(store)
}

pub fn serialize_store(store: &Store) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(store).expect("store serializes");
    bytes.push(b'\n');
    bytes
}

// ---------------------------------------------------------------- import

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportAction {
    /// No conflict — bring the project in as-is.
    Add,
    /// Drop the existing project of the same name; the incoming one wins.
    Replace,
    /// Bring the incoming project in under `new_name`.
    Rename,
    /// Leave this incoming project out entirely.
    Skip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDecision {
    /// The incoming project's name, exactly as previewed.
    pub name: String,
    pub action: ImportAction,
    pub new_name: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub skipped: usize,
}

/// Merge another store's projects into this one, one decision per
/// incoming project. Imported projects and snapshots get fresh ids so
/// the same file can be imported twice without id collisions; names,
/// history, and timestamps are preserved. Validates every decision
/// before touching the store — on error, nothing has changed.
pub fn merge_import(
    store: &mut Store,
    incoming: Store,
    decisions: &[ImportDecision],
) -> Result<ImportSummary, String> {
    use std::collections::HashSet;
    let norm = |s: &str| s.trim().to_lowercase();

    // A hand-edited file could carry blank or duplicate names; refuse
    // rather than guess which project the user meant.
    let mut seen: HashSet<String> = HashSet::new();
    for p in &incoming.projects {
        let n = norm(&p.name);
        if n.is_empty() {
            return Err("that store has a project with an empty name".into());
        }
        if !seen.insert(n) {
            return Err(format!(
                "that store has more than one project named \"{}\"",
                p.name.trim()
            ));
        }
    }

    struct Planned {
        project: Project,
        final_name: String,
    }

    let mut planned: Vec<Planned> = Vec::new();
    let mut replaced: HashSet<String> = HashSet::new();
    let mut summary = ImportSummary::default();

    for project in incoming.projects {
        let decision = decisions
            .iter()
            .find(|d| norm(&d.name) == norm(&project.name))
            .ok_or_else(|| {
                format!("no decision for incoming project \"{}\"", project.name.trim())
            })?;
        let exists = store.project_by_name(&project.name).is_some();
        let final_name = match decision.action {
            ImportAction::Skip => {
                summary.skipped += 1;
                continue;
            }
            ImportAction::Add => {
                if exists {
                    return Err(format!(
                        "a project named \"{}\" already exists — replace, rename, or skip it",
                        project.name.trim()
                    ));
                }
                summary.added += 1;
                project.name.trim().to_string()
            }
            ImportAction::Replace => {
                if !exists {
                    return Err(format!(
                        "nothing to replace — no project here is named \"{}\"",
                        project.name.trim()
                    ));
                }
                replaced.insert(norm(&project.name));
                summary.replaced += 1;
                project.name.trim().to_string()
            }
            ImportAction::Rename => {
                let new_name = decision
                    .new_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "give \"{}\" a new name, or choose replace or skip",
                            project.name.trim()
                        )
                    })?;
                summary.renamed += 1;
                new_name.to_string()
            }
        };
        planned.push(Planned {
            project,
            final_name,
        });
    }

    // After replacements, every surviving name — old and incoming —
    // must be unique.
    let mut taken: HashSet<String> = store
        .projects
        .iter()
        .map(|p| norm(&p.name))
        .filter(|n| !replaced.contains(n))
        .collect();
    for pl in &planned {
        if !taken.insert(norm(&pl.final_name)) {
            return Err(format!(
                "the name \"{}\" would collide with another project",
                pl.final_name
            ));
        }
    }

    store.projects.retain(|p| !replaced.contains(&norm(&p.name)));
    for pl in planned {
        let mut project = pl.project;
        project.id = new_id();
        project.name = pl.final_name;
        for s in &mut project.snapshots {
            s.id = new_id();
        }
        store.projects.push(project);
    }
    Ok(summary)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".bak")
}

/// Durable write: create the parent dir if missing, write a temp file +
/// fsync, then rename over the target. No backup — callers that want one
/// make it before calling. Used both for the store and for the one place
/// Envarsa writes into a project tree (a `.env*.local`), so a crash can
/// never leave a torn file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)
                .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
    }
    let tmp = sibling(path, ".tmp");
    {
        let mut f =
            fs::File::create(&tmp).map_err(|e| format!("could not write temp file: {e}"))?;
        f.write_all(bytes)
            .map_err(|e| format!("could not write temp file: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("could not flush temp file: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))?;
    Ok(())
}

/// Durable store write: back up the previous version, then atomically
/// replace the target.
pub fn write_store_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        fs::copy(path, backup_path(path))
            .map_err(|e| format!("could not write backup: {e}"))?;
    }
    write_atomic(path, bytes)
}

/// Serialize (and, when a passphrase is set, encrypt) the store, then
/// write it durably. Bumps `updated_at`.
pub fn save(store: &mut Store, path: &Path, passphrase: Option<&str>) -> Result<(), String> {
    store.updated_at = now_iso();
    let bytes = serialize_store(store);
    let bytes = match passphrase {
        Some(p) => crate::crypto::encrypt(&bytes, p)?,
        None => bytes,
    };
    write_store_file(path, &bytes)
}

/// Rewrite the backup with the live store's current bytes.
///
/// Protection transitions (encrypt, change passphrase, decrypt) call
/// this right after saving: a plain save preserves the *previous*
/// bytes as `.bak`, which after a transition is a copy under the old
/// protection — most notably a plaintext backup surviving the moment
/// the user encrypts the store. If the rewrite fails, the stale backup
/// is removed instead; only when both fail is an error returned.
pub fn align_backup(path: &Path) -> Result<(), String> {
    let bak = backup_path(path);
    if let Err(copy_err) = fs::copy(path, &bak) {
        match fs::remove_file(&bak) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "could not rewrite it ({copy_err}) or remove it ({e})"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("envarsa-test-{name}-{}", new_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn store_with_one_project() -> Store {
        let mut s = Store::new_empty();
        s.projects.push(Project {
            id: new_id(),
            name: "alpha".into(),
            path_hint: Some("C:\\dev\\alpha".into()),
            created_at: now_iso(),
            snapshots: vec![Snapshot {
                id: new_id(),
                captured_at: now_iso(),
                via: "paste".into(),
                source_path: None,
                raw: "A=1\nB=2\n".into(),
            }],
        });
        s
    }

    #[test]
    fn plaintext_roundtrip_and_backup() {
        let dir = tmp_dir("plain");
        let path = dir.join("envarsa.store");
        let mut s = store_with_one_project();

        save(&mut s, &path, None).unwrap();
        let loaded = parse_store(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name, "alpha");
        assert!(!backup_path(&path).exists(), "no backup on first write");

        save(&mut s, &path, None).unwrap();
        assert!(backup_path(&path).exists(), "backup exists after second write");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn encrypted_roundtrip() {
        let dir = tmp_dir("enc");
        let path = dir.join("envarsa.store");
        let mut s = store_with_one_project();

        save(&mut s, &path, Some("hunter2hunter2")).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(crate::crypto::is_encrypted(&bytes));
        let plain = crate::crypto::decrypt(&bytes, "hunter2hunter2").unwrap();
        let loaded = parse_store(&plain).unwrap();
        assert_eq!(loaded.projects[0].snapshots[0].raw, "A=1\nB=2\n");

        fs::remove_dir_all(dir).ok();
    }

    /// The backup must never retain bytes under weaker (or stale)
    /// protection than the live store after a protection transition:
    /// no plaintext .bak after encrypting, no old-passphrase .bak
    /// after re-keying.
    #[test]
    fn protection_transitions_realign_the_backup() {
        let dir = tmp_dir("align");
        let path = dir.join("envarsa.store");
        let bak = backup_path(&path);
        let mut s = store_with_one_project();

        // Plaintext history: two saves leave a plaintext .bak.
        save(&mut s, &path, None).unwrap();
        save(&mut s, &path, None).unwrap();
        assert!(!crate::crypto::is_encrypted(&fs::read(&bak).unwrap()));

        // Enable encryption: save + align must leave no plaintext copy.
        save(&mut s, &path, Some("first-pass-123")).unwrap();
        assert!(
            !crate::crypto::is_encrypted(&fs::read(&bak).unwrap()),
            "plain save alone keeps the plaintext backup — that is the hazard"
        );
        align_backup(&path).unwrap();
        let bak_bytes = fs::read(&bak).unwrap();
        assert!(crate::crypto::is_encrypted(&bak_bytes), "backup re-encrypted");
        assert!(crate::crypto::decrypt(&bak_bytes, "first-pass-123").is_ok());

        // Change passphrase: the backup must not stay readable with the
        // old one.
        save(&mut s, &path, Some("second-pass-456")).unwrap();
        align_backup(&path).unwrap();
        let bak_bytes = fs::read(&bak).unwrap();
        assert!(crate::crypto::decrypt(&bak_bytes, "first-pass-123").is_err());
        assert!(crate::crypto::decrypt(&bak_bytes, "second-pass-456").is_ok());

        // Disable: live and backup both go plaintext.
        save(&mut s, &path, None).unwrap();
        align_backup(&path).unwrap();
        assert!(!crate::crypto::is_encrypted(&fs::read(&bak).unwrap()));
        assert!(parse_store(&fs::read(&bak).unwrap()).is_ok());

        fs::remove_dir_all(dir).ok();
    }

    fn project(name: &str, raw: &str) -> Project {
        Project {
            id: new_id(),
            name: name.into(),
            path_hint: None,
            created_at: now_iso(),
            snapshots: vec![Snapshot {
                id: new_id(),
                captured_at: now_iso(),
                via: "paste".into(),
                source_path: None,
                raw: raw.into(),
            }],
        }
    }

    fn store_of(projects: Vec<Project>) -> Store {
        let mut s = Store::new_empty();
        s.projects = projects;
        s
    }

    fn decide(name: &str, action: ImportAction, new_name: Option<&str>) -> ImportDecision {
        ImportDecision {
            name: name.into(),
            action,
            new_name: new_name.map(String::from),
        }
    }

    #[test]
    fn merge_adds_skips_and_freshens_ids() {
        let mut mine = store_of(vec![project("alpha", "A=1\n")]);
        let incoming = store_of(vec![project("beta", "B=1\n"), project("gamma", "C=1\n")]);
        let in_beta_id = incoming.projects[0].id.clone();
        let in_beta_snap_id = incoming.projects[0].snapshots[0].id.clone();

        let sum = merge_import(
            &mut mine,
            incoming,
            &[
                decide("beta", ImportAction::Add, None),
                decide("gamma", ImportAction::Skip, None),
            ],
        )
        .unwrap();
        assert_eq!((sum.added, sum.skipped), (1, 1));
        assert_eq!(mine.projects.len(), 2);
        let beta = mine.project_by_name("beta").unwrap();
        assert_ne!(beta.id, in_beta_id, "imported project gets a fresh id");
        assert_ne!(beta.snapshots[0].id, in_beta_snap_id, "snapshots too");
        assert_eq!(beta.snapshots[0].raw, "B=1\n", "content preserved");
        assert!(mine.project_by_name("gamma").is_none(), "skipped stays out");
    }

    #[test]
    fn merge_replace_takes_the_incoming_version() {
        let mut mine = store_of(vec![project("alpha", "OLD=1\n")]);
        mine.project_mut(&mine.projects[0].id.clone())
            .unwrap()
            .snapshots
            .push(Snapshot {
                id: new_id(),
                captured_at: now_iso(),
                via: "paste".into(),
                source_path: None,
                raw: "OLD=2\n".into(),
            });
        let incoming = store_of(vec![project("Alpha", "NEW=1\n")]);

        let sum = merge_import(
            &mut mine,
            incoming,
            &[decide("Alpha", ImportAction::Replace, None)],
        )
        .unwrap();
        assert_eq!(sum.replaced, 1);
        assert_eq!(mine.projects.len(), 1);
        let p = mine.project_by_name("alpha").unwrap();
        assert_eq!(p.name, "Alpha", "incoming name wins on replace");
        assert_eq!(p.snapshots.len(), 1, "incoming history wins");
        assert_eq!(p.snapshots[0].raw, "NEW=1\n");
    }

    #[test]
    fn merge_rename_keeps_both() {
        let mut mine = store_of(vec![project("alpha", "A=1\n")]);
        let incoming = store_of(vec![project("alpha", "A=2\n")]);
        let sum = merge_import(
            &mut mine,
            incoming,
            &[decide("alpha", ImportAction::Rename, Some("alpha (imported)"))],
        )
        .unwrap();
        assert_eq!(sum.renamed, 1);
        assert_eq!(mine.projects.len(), 2);
        assert_eq!(
            mine.project_by_name("alpha (imported)").unwrap().snapshots[0].raw,
            "A=2\n"
        );
        assert_eq!(mine.project_by_name("alpha").unwrap().snapshots[0].raw, "A=1\n");
    }

    #[test]
    fn merge_rejects_bad_plans_without_mutating() {
        let mut mine = store_of(vec![project("alpha", "A=1\n"), project("beta", "B=1\n")]);

        // Add that still collides.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("ALPHA", "X=1\n")]),
            &[decide("ALPHA", ImportAction::Add, None)],
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");

        // Rename onto an existing name.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("alpha", "X=1\n")]),
            &[decide("alpha", ImportAction::Rename, Some("Beta"))],
        )
        .unwrap_err();
        assert!(err.contains("collide"), "got: {err}");

        // Two incoming projects renamed onto each other.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("one", "X=1\n"), project("two", "Y=1\n")]),
            &[
                decide("one", ImportAction::Add, None),
                decide("two", ImportAction::Rename, Some("one")),
            ],
        )
        .unwrap_err();
        assert!(err.contains("collide"), "got: {err}");

        // Replace with nothing to replace.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("gamma", "X=1\n")]),
            &[decide("gamma", ImportAction::Replace, None)],
        )
        .unwrap_err();
        assert!(err.contains("nothing to replace"), "got: {err}");

        // Missing decision.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("gamma", "X=1\n")]),
            &[],
        )
        .unwrap_err();
        assert!(err.contains("no decision"), "got: {err}");

        // Duplicate names inside the incoming file.
        let err = merge_import(
            &mut mine,
            store_of(vec![project("dup", "X=1\n"), project("DUP ", "Y=1\n")]),
            &[decide("dup", ImportAction::Add, None)],
        )
        .unwrap_err();
        assert!(err.contains("more than one project"), "got: {err}");

        // Nothing mutated by any of the failures.
        assert_eq!(mine.projects.len(), 2);
        assert_eq!(mine.project_by_name("alpha").unwrap().snapshots[0].raw, "A=1\n");
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert!(parse_store(b"{\"hello\":1}").is_err());
        let mut s = Store::new_empty();
        s.version = VERSION + 1;
        let bytes = serialize_store(&s);
        let err = parse_store(&bytes).unwrap_err();
        assert!(err.contains("newer"), "got: {err}");
    }
}
