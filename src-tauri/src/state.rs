//! Session state and boot-time resolution of the store location.
//!
//! Store path resolution order:
//!   1. `ENVARSA_STORE_PATH` env var (power users, tests)
//!   2. `store_path` in the app config file (set via Settings)
//!   3. the default location:
//!        - portable build (an `envarsa.portable` marker sits beside the
//!          exe): `<exe folder>/envarsa.store`, so the unzipped folder is
//!          self-contained and movable;
//!        - otherwise `<app data dir>/envarsa.store`.
//!
//! In a portable build `config.json` lives beside the exe too, so the
//! whole library — preferences included — travels as one folder.

use crate::store::{self, Opened, Snapshot, Store};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// User preferences, persisted as `config.json` in the app config dir.
/// Regenerable (unlike the store), so reads are forgiving. Saved
/// atomically, as a whole struct — a partial write here once
/// clobbered settings that other code paths had set.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,
    /// Opt-in: Envarsa never checks for updates unless this is true.
    #[serde(default)]
    pub auto_update_check: bool,
    /// Unix seconds of the last check attempt (manual or automatic),
    /// stamped before the request so failures can't retry-storm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<i64>,
    /// Last successful check's result; only meaningful while it is
    /// newer than the running version (status_of re-filters it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_version: Option<String>,
    /// Keys this build doesn't know about survive load/save round-trips.
    #[serde(flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

/// Name of the marker file a portable zip ships beside the exe. Its
/// presence — not its contents — is what flips Envarsa into portable mode.
const PORTABLE_MARKER: &str = "envarsa.portable";

fn has_portable_marker(dir: &Path) -> bool {
    dir.join(PORTABLE_MARKER).exists()
}

/// The folder a portable build runs from, when this is one. The portable
/// zip ships an `envarsa.portable` marker beside the exe; its presence
/// switches Envarsa to keeping both `config.json` and the store in that
/// folder, so the whole library travels with the unzipped folder.
/// Installed and Microsoft Store builds ship no marker and fall back to
/// the per-user AppData / config dirs.
pub fn portable_base() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    has_portable_marker(dir).then(|| dir.to_path_buf())
}

pub fn config_file_path(app: &tauri::AppHandle) -> PathBuf {
    if let Some(base) = portable_base() {
        return base.join("config.json");
    }
    app.path()
        .app_config_dir()
        .expect("app config dir resolves")
        .join("config.json")
}

/// A missing or unreadable config means defaults — it holds preferences,
/// never data.
pub fn load_config(config_path: &Path) -> Config {
    fs::read_to_string(config_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(config_path: &Path, config: &Config) -> Result<(), String> {
    let body = serde_json::to_string_pretty(config)
        .map_err(|e| format!("could not encode config: {e}"))?;
    store::write_atomic(config_path, body.as_bytes())
        .map_err(|e| format!("could not write config: {e}"))
}

pub enum Session {
    Unlocked {
        store: Store,
        /// Present when encryption-at-rest is enabled; saves re-encrypt.
        passphrase: Option<String>,
    },
    Locked,
    Corrupt {
        error: String,
    },
}

impl Session {
    /// The store, or why there isn't one to use.
    #[allow(dead_code)] // Not yet called from commands.rs.
    pub fn unlocked(&self) -> Result<&Store, String> {
        match self {
            Session::Unlocked { store, .. } => Ok(store),
            Session::Locked => Err("the store is locked".into()),
            Session::Corrupt { error } => Err(format!("the store could not be loaded: {error}")),
        }
    }

    /// The store and its passphrase, for changing either.
    pub fn unlocked_mut(&mut self) -> Result<(&mut Store, &mut Option<String>), String> {
        match self {
            Session::Unlocked { store, passphrase } => Ok((store, passphrase)),
            Session::Locked => Err("the store is locked".into()),
            Session::Corrupt { error } => Err(format!("the store could not be loaded: {error}")),
        }
    }
}

/// A store file the user picked for import. The dialog command mints
/// the token and keeps the path here, on the Rust side — inspect/apply
/// accept only the token, so the webview never supplies a path.
pub struct PendingImport {
    pub token: String,
    pub path: PathBuf,
}

/// A `.env*.local` write target the user is about to commit to. Like
/// `PendingImport`, the path stays here on the Rust side — the write
/// command takes only the token, so the webview never supplies a path.
/// `template`, when set, is an imported `.env.example`'s text used as the
/// scaffold for the merge; the example file itself is only ever read.
pub struct PendingWrite {
    pub token: String,
    pub path: PathBuf,
    pub template: Option<String>,
}

pub struct Inner {
    pub store_path: PathBuf,
    pub config_path: PathBuf,
    pub config: Config,
    /// True when ENVARSA_STORE_PATH is in effect — relocating the store
    /// through Settings is refused then, because the env var would win
    /// again on the next start.
    pub env_override: bool,
    pub session: Session,
    /// At most one import is in flight; a new pick replaces it.
    pub pending_import: Option<PendingImport>,
    /// The write modal's two tabs each keep one staged `.env.local`
    /// write: a plain target, and an example scaffold (`template` set).
    /// A new pick replaces only its own kind, so choosing an example
    /// never un-stages the target picked on the other tab.
    pub pending_target: Option<PendingWrite>,
    pub pending_example: Option<PendingWrite>,
}

impl Inner {
    /// Stage a write and return its token.
    pub fn stage_write(&mut self, path: PathBuf, template: Option<String>) -> String {
        let token = store::new_id();
        let slot = if template.is_some() {
            &mut self.pending_example
        } else {
            &mut self.pending_target
        };
        *slot = Some(PendingWrite {
            token: token.clone(),
            path,
            template,
        });
        token
    }

    pub fn pending_write(&self, token: &str) -> Option<&PendingWrite> {
        [&self.pending_target, &self.pending_example]
            .into_iter()
            .flatten()
            .find(|p| p.token == token)
    }

    /// A finished write closes the modal, so nothing it staged stays valid.
    pub fn clear_pending_writes(&mut self) {
        self.pending_target = None;
        self.pending_example = None;
    }
}

#[derive(Default)]
pub struct AppState(pub Mutex<Option<Inner>>);

impl AppState {
    /// Run `f` with the app state locked.
    pub fn with<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T, String>) -> Result<T, String> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| "internal: state poisoned".to_string())?;
        let inner = guard
            .as_mut()
            .ok_or_else(|| "app is still starting".to_string())?;
        f(inner)
    }
}

pub fn resolve_store_path(app: &tauri::AppHandle, config: &Config) -> (PathBuf, bool) {
    let default = match portable_base() {
        Some(base) => base.join("envarsa.store"),
        None => app
            .path()
            .app_data_dir()
            .expect("app data dir resolves")
            .join("envarsa.store"),
    };
    resolve_store_path_with(
        std::env::var("ENVARSA_STORE_PATH").ok(),
        config.store_path.as_deref(),
        default,
    )
}

/// The precedence, factored out so it can be tested without a Tauri app:
/// the `ENVARSA_STORE_PATH` env var (which also locks relocation), then the
/// Settings-chosen path, then the default the caller computed. Blank values
/// are treated as unset and fall through.
fn resolve_store_path_with(
    env_path: Option<String>,
    config_path: Option<&str>,
    default: PathBuf,
) -> (PathBuf, bool) {
    if let Some(p) = env_path {
        if !p.trim().is_empty() {
            return (PathBuf::from(p), true);
        }
    }

    if let Some(p) = config_path {
        if !p.trim().is_empty() {
            return (PathBuf::from(p), false);
        }
    }

    (default, false)
}

/// Load the store file into a session. A missing file means first run:
/// a fresh plaintext store is created immediately so the library is
/// durable from second zero.
pub fn init_session(store_path: &Path) -> Session {
    match fs::read(store_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut s = Store::new_empty();
            match store::save(&mut s, store_path, None) {
                Ok(()) => Session::Unlocked {
                    store: s,
                    passphrase: None,
                },
                Err(error) => Session::Corrupt { error },
            }
        }
        Err(e) => Session::Corrupt {
            error: format!("could not read store file: {e}"),
        },
        Ok(bytes) => match store::open(&bytes, None) {
            Ok(Opened::Plain(s)) => Session::Unlocked {
                store: s,
                passphrase: None,
            },
            Ok(Opened::NeedsPassphrase | Opened::Encrypted(_)) => Session::Locked,
            Err(error) => Session::Corrupt { error },
        },
    }
}

/// The sample library ENVARSA_DEMO seeds, stored as an ordinary store
/// file. Ids and timestamps are minted fresh on each load.
fn demo_projects() -> Vec<store::Project> {
    let demo = store::parse_store(include_str!("../demo.json").as_bytes())
        .expect("demo.json is a valid store");
    demo.projects
        .into_iter()
        .map(|mut p| {
            p.id = store::new_id();
            p.created_at = store::now_iso();
            for s in &mut p.snapshots {
                *s = Snapshot::new(&s.via, s.source_path.take(), std::mem::take(&mut s.raw));
            }
            p
        })
        .collect()
}

/// When ENVARSA_DEMO is set and the store is empty, seed a few sample
/// projects. Used for screenshots and trying the app out — never runs
/// against a store that already has data.
pub fn maybe_seed_demo(session: &mut Session, store_path: &Path) {
    if std::env::var("ENVARSA_DEMO").is_err() {
        return;
    }
    let Ok((store, passphrase)) = session.unlocked_mut() else {
        return;
    };
    if !store.projects.is_empty() {
        return;
    }
    store.projects = demo_projects();
    let pass = passphrase.clone();
    let _ = store::save(store, store_path, pass.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::{project, store_of, tmp_dir};

    #[test]
    fn missing_or_corrupt_config_means_defaults() {
        let dir = tmp_dir("cfg-missing");
        let path = dir.join("config.json");

        let c = load_config(&path);
        assert!(c.store_path.is_none());
        assert!(!c.auto_update_check, "update checks must be opt-in");
        assert!(c.last_update_check.is_none());
        assert!(c.available_version.is_none());

        fs::write(&path, "{ not json").unwrap();
        let c = load_config(&path);
        assert!(c.store_path.is_none());
        assert!(!c.auto_update_check);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_store_path_only_config_loads() {
        let dir = tmp_dir("cfg-legacy");
        let path = dir.join("config.json");
        fs::write(&path, r#"{ "store_path": "D:\\sync\\envarsa.store" }"#).unwrap();

        let c = load_config(&path);
        assert_eq!(c.store_path.as_deref(), Some("D:\\sync\\envarsa.store"));
        assert!(!c.auto_update_check);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn roundtrip_keeps_all_fields_and_unknown_keys() {
        let dir = tmp_dir("cfg-roundtrip");
        let path = dir.join("config.json");
        fs::write(
            &path,
            r#"{ "store_path": "X", "from_the_future": { "keep": true } }"#,
        )
        .unwrap();

        let mut c = load_config(&path);
        c.auto_update_check = true;
        c.last_update_check = Some(1_765_540_000);
        c.available_version = Some("0.2.0".into());
        save_config(&path, &c).unwrap();

        let back = load_config(&path);
        assert_eq!(back.store_path.as_deref(), Some("X"));
        assert!(back.auto_update_check);
        assert_eq!(back.last_update_check, Some(1_765_540_000));
        assert_eq!(back.available_version.as_deref(), Some("0.2.0"));
        assert!(
            back.rest.contains_key("from_the_future"),
            "unknown keys must survive a round-trip"
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn env_var_wins_and_locks_relocation() {
        let (p, locked) = resolve_store_path_with(
            Some("D:\\sync\\envarsa.store".into()),
            Some("C:\\settings\\envarsa.store"),
            PathBuf::from("C:\\default\\envarsa.store"),
        );
        assert_eq!(p, PathBuf::from("D:\\sync\\envarsa.store"));
        assert!(locked, "ENVARSA_STORE_PATH must lock relocation");
    }

    #[test]
    fn settings_path_wins_over_default() {
        let (p, locked) = resolve_store_path_with(
            None,
            Some("C:\\settings\\envarsa.store"),
            PathBuf::from("C:\\default\\envarsa.store"),
        );
        assert_eq!(p, PathBuf::from("C:\\settings\\envarsa.store"));
        assert!(!locked);
    }

    #[test]
    fn default_is_used_when_nothing_set() {
        let default = PathBuf::from("C:\\default\\envarsa.store");
        let (p, locked) = resolve_store_path_with(None, None, default.clone());
        assert_eq!(p, default);
        assert!(!locked);
    }

    #[test]
    fn blank_env_and_settings_fall_through_to_default() {
        // A blank or whitespace value is treated as unset, including the
        // portable default the caller computed.
        let default = PathBuf::from("C:\\portable\\envarsa.store");
        let (p, locked) = resolve_store_path_with(Some("   ".into()), Some(" "), default.clone());
        assert_eq!(p, default);
        assert!(!locked);
    }

    #[test]
    fn portable_marker_detected_only_when_present() {
        let dir = tmp_dir("portable");
        assert!(!has_portable_marker(&dir), "no marker → not portable");
        fs::write(dir.join(PORTABLE_MARKER), b"").unwrap();
        assert!(has_portable_marker(&dir), "marker beside exe → portable");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn save_config_creates_the_folder_and_leaves_no_temp_file() {
        let dir = tmp_dir("cfg-atomic");
        let path = dir.join("nested").join("config.json");
        let c = Config {
            auto_update_check: true,
            ..Config::default()
        };
        save_config(&path, &c).unwrap();
        assert!(load_config(&path).auto_update_check);
        assert!(!dir.join("nested").join("config.json.tmp").exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn session_access_maps_locked_and_corrupt_to_errors() {
        let mut s = Session::Unlocked {
            store: store_of(vec![project("alpha", "A=1\n")]),
            passphrase: None,
        };
        assert_eq!(s.unlocked().unwrap().projects.len(), 1);
        *s.unlocked_mut().unwrap().1 = Some("pass-phrase".into());
        assert!(matches!(
            &s,
            Session::Unlocked {
                passphrase: Some(_),
                ..
            }
        ));

        let mut locked = Session::Locked;
        assert_eq!(locked.unlocked().err().unwrap(), "the store is locked");
        assert!(locked.unlocked_mut().is_err());
        let corrupt = Session::Corrupt {
            error: "bad json".into(),
        };
        assert_eq!(
            corrupt.unlocked().err().unwrap(),
            "the store could not be loaded: bad json"
        );
    }

    #[test]
    fn app_state_with_reports_a_missing_inner() {
        let state = AppState::default();
        let err = state.with(|_| Ok(())).unwrap_err();
        assert_eq!(err, "app is still starting");
        *state.0.lock().unwrap() = Some(inner());
        assert!(state.with(|i| Ok(i.env_override)).is_ok_and(|o| !o));
    }

    #[test]
    fn init_session_creates_opens_and_locks() {
        let dir = tmp_dir("init");
        let path = dir.join("envarsa.store");
        assert!(matches!(init_session(&path), Session::Unlocked { .. }));
        assert!(path.exists(), "first run writes a store at once");

        let mut s = store_of(vec![project("alpha", "A=1\n")]);
        store::save(&mut s, &path, Some("hunter2hunter2")).unwrap();
        assert!(matches!(init_session(&path), Session::Locked));

        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(init_session(&path), Session::Corrupt { .. }));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn demo_seed_matches_the_sample_library() {
        let projects = demo_projects();
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["lumen-api", "lumen-web", "tooling-scripts"]);
        let api = &projects[0];
        assert_eq!(api.path_hint.as_deref(), Some("C:\\dev\\lumen\\api"));
        assert_eq!(api.snapshots.len(), 2);
        assert_eq!(
            api.snapshots[1].source_path.as_deref(),
            Some("C:\\dev\\lumen\\api\\.env")
        );
        assert!(api.snapshots[1].raw.contains("JWT_SECRET="));
        assert_eq!(projects[2].snapshots[0].via, "paste");
        assert_eq!(projects[2].snapshots[0].source_path, None);
        assert_ne!(
            demo_projects()[0].id,
            api.id,
            "each seeding mints fresh ids"
        );
    }

    fn inner() -> Inner {
        Inner {
            store_path: PathBuf::from("envarsa.store"),
            config_path: PathBuf::from("config.json"),
            config: Config::default(),
            env_override: false,
            session: Session::Locked,
            pending_import: None,
            pending_target: None,
            pending_example: None,
        }
    }

    /// The write modal's two tabs stage independently: picking on one
    /// tab must not invalidate the token the other tab holds.
    #[test]
    fn target_and_example_writes_stay_staged_side_by_side() {
        let mut i = inner();
        let target = i.stage_write(PathBuf::from("/app/.env.local"), None);
        let example = i.stage_write(PathBuf::from("/tpl/.env.local"), Some("A=\n".into()));
        assert_eq!(
            i.pending_write(&target).map(|p| p.path.clone()),
            Some(PathBuf::from("/app/.env.local")),
            "picking an example keeps the staged target"
        );
        let template = i
            .pending_write(&example)
            .and_then(|p| p.template.as_deref());
        assert_eq!(template, Some("A=\n"));

        // The other order: re-staging the target keeps the example, and
        // replaces only the previous target.
        let target2 = i.stage_write(PathBuf::from("/other/.env.local"), None);
        assert!(i.pending_write(&example).is_some(), "example kept");
        assert!(i.pending_write(&target).is_none(), "old target replaced");
        assert!(i.pending_write(&target2).is_some());
        assert!(i.pending_write("not-a-token").is_none());
    }

    #[test]
    fn a_finished_write_clears_every_staged_write() {
        let mut i = inner();
        let target = i.stage_write(PathBuf::from("/app/.env.local"), None);
        let example = i.stage_write(PathBuf::from("/tpl/.env.local"), Some(String::new()));
        i.clear_pending_writes();
        assert!(i.pending_write(&target).is_none());
        assert!(i.pending_write(&example).is_none());
    }
}
