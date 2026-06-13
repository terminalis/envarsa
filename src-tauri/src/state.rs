//! Session state and boot-time resolution of the store location.
//!
//! Store path resolution order:
//!   1. `ENVARSA_STORE_PATH` env var (power users, tests)
//!   2. `store_path` in the app config file (set via Settings)
//!   3. `<app data dir>/envarsa.store` (default)

use crate::store::{self, Project, Snapshot, Store};
use crate::crypto;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// User preferences, persisted as `config.json` in the app config dir.
/// Regenerable (unlike the store), so reads are forgiving and writes
/// are plain. Saved as a whole struct — a partial write here once
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

pub fn config_file_path(app: &tauri::AppHandle) -> PathBuf {
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
    if let Some(dir) = config_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("could not create config dir: {e}"))?;
    }
    let body = serde_json::to_string_pretty(config)
        .map_err(|e| format!("could not encode config: {e}"))?;
    fs::write(config_path, body).map_err(|e| format!("could not write config: {e}"))
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
    /// At most one `.env.local` write is staged; a new pick replaces it.
    pub pending_write: Option<PendingWrite>,
}

#[derive(Default)]
pub struct AppState(pub Mutex<Option<Inner>>);

pub fn resolve_store_path(app: &tauri::AppHandle, config: &Config) -> (PathBuf, bool) {
    if let Ok(p) = std::env::var("ENVARSA_STORE_PATH") {
        if !p.trim().is_empty() {
            return (PathBuf::from(p), true);
        }
    }

    if let Some(p) = config.store_path.as_deref() {
        if !p.trim().is_empty() {
            return (PathBuf::from(p), false);
        }
    }

    let data_dir = app.path().app_data_dir().expect("app data dir resolves");
    (data_dir.join("envarsa.store"), false)
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
        Ok(bytes) => {
            if crypto::is_encrypted(&bytes) {
                Session::Locked
            } else {
                match store::parse_store(&bytes) {
                    Ok(s) => Session::Unlocked {
                        store: s,
                        passphrase: None,
                    },
                    Err(error) => Session::Corrupt { error },
                }
            }
        }
    }
}

/// When ENVARSA_DEMO is set and the store is empty, seed a few sample
/// projects. Used for screenshots and trying the app out — never runs
/// against a store that already has data.
pub fn maybe_seed_demo(session: &mut Session, store_path: &Path) {
    if std::env::var("ENVARSA_DEMO").is_err() {
        return;
    }
    let Session::Unlocked { store, passphrase } = session else {
        return;
    };
    if !store.projects.is_empty() {
        return;
    }

    let mut add = |name: &str, hint: &str, raws: &[(&str, &str)]| {
        let snapshots = raws
            .iter()
            .map(|(via, raw)| Snapshot {
                id: store::new_id(),
                captured_at: store::now_iso(),
                via: via.to_string(),
                source_path: if *via == "file" {
                    Some(format!("{hint}\\.env"))
                } else {
                    None
                },
                raw: raw.to_string(),
            })
            .collect();
        store.projects.push(Project {
            id: store::new_id(),
            name: name.to_string(),
            path_hint: Some(hint.to_string()),
            created_at: store::now_iso(),
            snapshots,
        });
    };

    add(
        "lumen-api",
        "C:\\dev\\lumen\\api",
        &[
            (
                "file",
                "# Server\nPORT=3000\nLOG_LEVEL=info\n\n# Database\nDATABASE_URL=postgres://lumen:s3cr3t@localhost:5432/lumen_dev\nREDIS_URL=redis://localhost:6379/0\n",
            ),
            (
                "file",
                "# Server\nPORT=3000\nLOG_LEVEL=debug\n\n# Database\nDATABASE_URL=postgres://lumen:s3cr3t@localhost:5432/lumen_dev\nREDIS_URL=redis://localhost:6379/0\n\n# Auth\nJWT_SECRET=9f1c4f5a2e6b48d3a7c0e9b1d8f24a61\nSESSION_TTL_HOURS=72\n\n# Stripe (test mode)\nSTRIPE_SECRET_KEY=sk_test_demo-not-a-real-key\nSTRIPE_WEBHOOK_SECRET=whsec_8a2f0d9c1b3e4f5a6d7c8b9a0e1f2d3c\n\n# Observability\nSENTRY_DSN=https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247\n",
            ),
        ],
    );
    add(
        "lumen-web",
        "C:\\dev\\lumen\\web",
        &[(
            "file",
            "VITE_API_URL=http://localhost:3000\nVITE_STRIPE_PUBLISHABLE_KEY=pk_test_TYooMQauvdEDq54NiTphI7jx\nLOG_LEVEL=warn\nSENTRY_DSN=https://e1f2a3b4c5d6@o447951.ingest.sentry.io/5901247\n",
        )],
    );
    add(
        "tooling-scripts",
        "C:\\dev\\tooling",
        &[(
            "paste",
            "# Personal automation\nGITHUB_TOKEN=ghp_demo-not-a-real-token\nOPENAI_API_KEY=sk-proj-demo-not-a-real-key-000000000000\nDATABASE_URL=postgres://tools:tools@localhost:5432/scratch\n",
        )],
    );

    let pass = passphrase.clone();
    let _ = store::save(store, store_path, pass.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("envarsa-test-{name}-{}", store::new_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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
}
