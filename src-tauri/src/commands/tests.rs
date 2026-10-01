//! The IPC commands end to end, over a real store in a temp folder.
//! Tauri's mock app manages the state, so each test calls the same
//! command functions the webview invokes, with arguments decoded from
//! the JSON the webview sends. Native dialogs need a person, so tests
//! stage the path the user would have picked with the same function the
//! dialog command calls.

use super::library::CaptureResult;
use super::{export, library, secrets, session, transfer};
use crate::crypto;
use crate::envfile;
use crate::state::{self, AppState, Config, Inner};
use crate::store::{self, Store};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::test::MockRuntime;
use tauri::{App, Manager, State};

const ALPHA: &str = "# Database\nDATABASE_URL=\"postgres://u:p@localhost/db\"\nREDIS_URL=redis://localhost:6379\nEMPTY=\nexport REGION=eu-west-1\nDUP=1\nDUP=2\n";
const BETA: &str =
    "DATABASE_URL=\"postgres://u:p@localhost/db\"\nREDIS_URL=redis://other:6379\nONLY_BETA=x\n";
/// Chosen so that re-serializing its parsed lines gives the same bytes.
const WRITE: &str =
    "# App\nPORT=3000\nexport REGION=eu-west-1\n\n# Secrets\nAPI_KEY=real-secret\nTOKEN=t0ken\n";
const TRANSPORT: &str = "transport-pass-123";

/// A running app over a fresh store in its own temp folder.
struct Env {
    app: App<MockRuntime>,
    dir: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let dir = store::fixtures::tmp_dir(name);
        let store_path = dir.join("envarsa.store");
        let session = state::init_session(&store_path);
        let config = dir.join("config.json");
        let inner = Inner::new(store_path, config, Config::default(), false, session);
        let app = tauri::test::mock_app();
        app.manage(AppState(Mutex::new(Some(inner))));
        Env { app, dir }
    }

    fn state(&self) -> State<'_, AppState> {
        self.app.state()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn store_path(&self) -> PathBuf {
        self.path("envarsa.store")
    }

    /// The store as saved on disk (plaintext stores only).
    fn on_disk(&self) -> Store {
        store::parse_store(&fs::read(self.store_path()).unwrap()).unwrap()
    }

    /// Paste `text` into the project called `name`.
    fn capture(&self, name: &str, text: &str) -> CaptureResult {
        let args = decode(json!({ "projectName": name, "text": text }));
        library::capture(self.state(), args).unwrap()
    }

    fn status(&self) -> Value {
        to_json(&session::store_status(self.state()).unwrap())
    }

    fn projects(&self) -> Vec<Value> {
        let list = to_json(&library::list_projects(self.state()).unwrap());
        list.as_array().unwrap().clone()
    }

    fn project(&self, name: &str) -> Value {
        self.projects()
            .into_iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no project named {name}"))
    }

    fn view(&self, project_id: &str, snapshot_id: Option<&str>) -> Value {
        let view = library::get_project(
            self.state(),
            project_id.into(),
            snapshot_id.map(String::from),
        );
        to_json(&view.unwrap())
    }

    fn reveal(&self, project_id: &str, snapshot_id: &str, idx: usize) -> String {
        secrets::reveal_value(self.state(), project_id.into(), snapshot_id.into(), idx)
            .unwrap()
            .value
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

/// A command's error, when it must refuse (`unwrap_err` would need
/// `Debug` on every response type).
trait Refused {
    fn refused(self) -> String;
}

impl<T> Refused for Result<T, String> {
    fn refused(self) -> String {
        match self {
            Ok(_) => panic!("expected the command to refuse"),
            Err(e) => e,
        }
    }
}

/// Decode command arguments from the JSON the webview would send.
fn decode<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

/// Encode a response as the JSON the webview receives.
fn to_json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

fn lines(view: &Value) -> &Vec<Value> {
    view["lines"].as_array().unwrap()
}

fn line<'a>(view: &'a Value, key: &str) -> &'a Value {
    lines(view).iter().find(|l| l["key"] == key).unwrap()
}

fn idx(line: &Value) -> usize {
    line["idx"].as_u64().unwrap() as usize
}

// ------------------------------------------------------ status & capture

#[test]
fn a_fresh_store_is_unlocked_and_empty() {
    let env = Env::new("cmd-fresh");
    let st = env.status();
    assert_eq!(st["state"], "unlocked");
    assert_eq!(st["projectCount"], 0);
    assert_eq!(st["encrypted"], false);
    assert_eq!(st["storePath"], env.store_path().to_string_lossy().as_ref());
    assert_eq!(st["appVersion"], env!("CARGO_PKG_VERSION"));
    // A test build has no package identity and runs outside a Flatpak.
    assert_eq!(st["channel"], "direct");
    assert_eq!(st["customLocation"], false);
}

#[test]
fn a_capture_preview_counts_entries_comments_problems_and_duplicates() {
    let preview = library::preview_capture("A=1\n# c\nA=2\nBAD LINE".into()).unwrap();
    assert_eq!(
        to_json(&preview),
        json!({ "entries": 1, "comments": 1, "bad": 1, "dupKeys": ["A"] })
    );
}

#[test]
fn a_capture_creates_a_project_with_one_snapshot() {
    let env = Env::new("cmd-capture");
    let args = json!({ "projectName": "alpha", "pathHint": "/work/alpha", "text": ALPHA });
    let r = library::capture(env.state(), decode(args)).unwrap();
    assert_eq!(r.entry_count, 5);
    let list = env.projects();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "alpha");
    assert_eq!(list[0]["pathHint"], "/work/alpha");
    assert_eq!(list[0]["entryCount"], 5);
    assert_eq!(list[0]["snapshotCount"], 1);
    assert_eq!(env.view(&r.project_id, None)["via"], "paste");
}

#[test]
fn captures_find_their_project_by_name() {
    let env = Env::new("cmd-by-name");
    let first = env.capture("alpha", "A=1\n");
    let again = env.capture("  ALPHA ", "A=2\n");
    assert_eq!(again.project_id, first.project_id);
    assert_eq!(env.projects().len(), 1);
    assert_eq!(env.project("alpha")["snapshotCount"], 2);

    let blank = decode(json!({ "projectName": "  ", "text": "A=1\n" }));
    let err = library::capture(env.state(), blank).refused();
    assert_eq!(err, "give the project a name");
}

#[test]
fn a_picked_file_is_captured_by_token_never_by_path() {
    let env = Env::new("cmd-source");
    let dir = env.path("web");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(".env");
    fs::write(&file, "PORT=1\n").unwrap();

    let picked =
        to_json(&library::stage_env_file(&env.state(), &file, Some(file.clone())).unwrap());
    assert_eq!(picked["text"], "PORT=1\n");
    assert_eq!(picked["nameGuess"], "web");
    let token = picked["token"].as_str().unwrap();
    let args = json!({ "projectName": "web", "text": "PORT=1\n", "sourceToken": token });
    let r = library::capture(env.state(), decode(args)).unwrap();
    assert_eq!(env.view(&r.project_id, None)["via"], "file");
    let saved = env.on_disk();
    let source = saved.projects[0].latest().unwrap().source_path.clone();
    assert_eq!(source.as_deref(), Some(file.to_string_lossy().as_ref()));

    // The source decides where a write goes by default, so only a token
    // the core minted is accepted, and a new pick replaces the last one.
    library::stage_env_file(&env.state(), &file, Some(file.clone())).unwrap();
    let args = json!({ "projectName": "web", "text": "A=1\n", "sourceToken": token });
    let err = library::capture(env.state(), decode(args)).refused();
    assert!(err.contains("no longer staged"), "{err}");
    assert_eq!(env.project("web")["snapshotCount"], 1, "nothing captured");
}

/// In the Flatpak a picked file is read through the Documents portal,
/// and its host path is what gets recorded. When the portal can't say,
/// the file is still captured, with no source and no guesses.
#[test]
fn a_file_with_no_known_host_path_is_captured_without_a_source() {
    let env = Env::new("cmd-source-unknown");
    let file = env.path(".env");
    fs::write(&file, "PORT=1\n").unwrap();

    let picked = to_json(&library::stage_env_file(&env.state(), &file, None).unwrap());
    assert_eq!(
        picked["path"],
        file.to_string_lossy().as_ref(),
        "shown as read"
    );
    assert_eq!(picked["dir"], Value::Null);
    assert_eq!(picked["nameGuess"], Value::Null);
    let args = json!({ "projectName": "web", "text": "PORT=1\n", "sourceToken": picked["token"] });
    let r = library::capture(env.state(), decode(args)).unwrap();
    assert_eq!(env.view(&r.project_id, None)["via"], "file");
    assert_eq!(
        env.on_disk().projects[0].latest().unwrap().source_path,
        None
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_documents_portal_path_names_its_document() {
    use std::path::Path;
    fn id(p: &str) -> Option<&str> {
        super::doc_id(Path::new(p))
    }
    assert_eq!(id("/run/user/1000/doc/ab12cd34/.env"), Some("ab12cd34"));
    assert_eq!(id("/home/me/app/.env"), None);
    assert_eq!(
        id("/run/user/1000/doc/ab12cd34"),
        None,
        "a document, not a file"
    );
    assert_eq!(id("/run/user/1000/doc/ab12cd34/app/.env"), None);
    assert_eq!(id("/run/user/1000/other/ab12cd34/.env"), None);
}

// ------------------------------------------------------- listing & reveal

#[test]
fn a_project_view_has_structure_but_never_values() {
    let env = Env::new("cmd-view");
    let a = env.capture("alpha", ALPHA);
    let v = env.view(&a.project_id, None);
    assert_eq!(v["isLatest"], true);
    // A trailing newline ends the last line; there is no phantom blank.
    assert_eq!(lines(&v).len(), 7);
    assert_eq!(lines(&v)[0]["t"], "comment");
    assert_eq!(lines(&v)[1]["t"], "entry");
    assert_eq!(lines(&v)[1]["key"], "DATABASE_URL");
    assert!(lines(&v).iter().all(|l| l.get("value").is_none()));
    assert!(!v.to_string().contains("postgres://"), "no value anywhere");
    assert_eq!(lines(&v)[4]["exported"], true);
    assert_eq!(
        lines(&v)[5]["overridden"],
        true,
        "the first DUP is overridden"
    );
    assert_eq!(lines(&v)[6]["overridden"], false, "the last DUP wins");
}

#[test]
fn reveal_returns_unquoted_values_and_masked_bad_lines() {
    let env = Env::new("cmd-reveal");
    let a = env.capture("alpha", ALPHA);
    let r = secrets::reveal_value(env.state(), a.project_id.clone(), a.snapshot_id.clone(), 1);
    let r = r.unwrap();
    assert_eq!(
        (r.key.as_str(), r.value.as_str()),
        ("DATABASE_URL", "postgres://u:p@localhost/db")
    );
    let comment = secrets::reveal_value(env.state(), a.project_id, a.snapshot_id, 0);
    assert_eq!(comment.refused(), "that line has no value to reveal");

    // A malformed line may hold a secret: masked in the listing, raw
    // text only on reveal.
    let g = env.capture(
        "badline",
        "GOOD=1\nAuthorization: Bearer not-a-real-token\n",
    );
    let v = env.view(&g.project_id, None);
    let bad = lines(&v).iter().find(|l| l["t"] == "bad").unwrap();
    assert!(bad.get("raw").is_none(), "no raw text in the listing");
    assert_eq!(
        env.reveal(&g.project_id, &g.snapshot_id, idx(bad)),
        "Authorization: Bearer not-a-real-token"
    );
}

#[test]
fn keys_reused_across_projects_are_flagged_as_same_or_different() {
    let env = Env::new("cmd-reuse");
    let a = env.capture("alpha", ALPHA);
    let b = env.capture("beta", BETA);
    let v = env.view(&a.project_id, None);
    let db = &line(&v, "DATABASE_URL")["reuse"];
    assert_eq!(
        db,
        &json!([{ "projectId": b.project_id, "name": "beta", "same": true }])
    );
    assert_eq!(line(&v, "REDIS_URL")["reuse"][0]["same"], false);
    assert_eq!(line(&v, "EMPTY")["reuse"], json!([]));
    assert_eq!(env.project("alpha")["sharedKeys"], 2);
    assert_eq!(env.project("beta")["sharedKeys"], 2);
}

/// Copying hands a value from the core straight to the OS clipboard.
/// Needs a desktop session, so it is ignored by default:
/// `cargo test -- --ignored copy`. On Linux Envarsa serves the copy
/// itself, so it is gone once the test exits.
#[test]
#[ignore = "needs a desktop clipboard"]
fn copy_puts_the_value_or_the_raw_block_on_the_clipboard() {
    // This thread stands in for the app's main thread, where GTK runs.
    #[cfg(target_os = "linux")]
    gtk::init().unwrap();
    let env = Env::new("cmd-copy");
    let a = env.capture("alpha", ALPHA);
    let b = env.capture("beta", BETA);
    #[cfg(windows)]
    let original = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .ok();

    let key = secrets::copy_value(env.state(), a.project_id, a.snapshot_id, 1).unwrap();
    assert_eq!(key, "DATABASE_URL");
    assert_eq!(pasted(), "postgres://u:p@localhost/db");
    let n = secrets::copy_block(env.state(), b.project_id, b.snapshot_id).unwrap();
    assert_eq!(n, 3);
    assert_eq!(
        pasted(),
        BETA,
        "the block is the raw snapshot, byte for byte"
    );

    #[cfg(windows)]
    if let Some(text) = original {
        let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text));
    }
}

/// The clipboard's text, read the way another app would paste it.
#[cfg(windows)]
fn pasted() -> String {
    arboard::Clipboard::new().unwrap().get_text().unwrap()
}

#[cfg(target_os = "linux")]
fn pasted() -> String {
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clipboard.wait_for_text().unwrap().into()
}

#[test]
fn an_export_is_the_raw_snapshot_byte_for_byte() {
    let env = Env::new("cmd-export");
    let b = env.capture("beta", BETA);
    let (raw, name) = export::snapshot_export(&env.state(), &b.project_id, &b.snapshot_id).unwrap();
    assert_eq!(raw, BETA);
    assert_eq!(name, "beta.env");
}

// -------------------------------------------------------- project edits

#[test]
fn update_project_renames_and_sets_the_folder_in_one_save() {
    let env = Env::new("cmd-update");
    env.capture("alpha", "A=1\n");
    let b = env.capture("beta", "B=1\n");
    let update = |name: &str, hint: &str| {
        library::update_project(env.state(), b.project_id.clone(), name.into(), hint.into())
    };

    update(" renamed ", " /work/b ").unwrap();
    let saved = env.on_disk();
    let p = saved.project(&b.project_id).unwrap();
    assert_eq!(
        (p.name.as_str(), p.path_hint.as_deref()),
        ("renamed", Some("/work/b"))
    );

    // A taken name refuses the whole edit: the folder doesn't change either.
    let err = update("ALPHA", "/elsewhere").refused();
    assert_eq!(err, "a project named \"alpha\" already exists");
    let v = env.view(&b.project_id, None);
    assert_eq!(
        (&v["name"], &v["pathHint"]),
        (&json!("renamed"), &json!("/work/b"))
    );

    // Its own name in another case is fine; a blank folder clears it.
    update("Renamed", "  ").unwrap();
    let v = env.view(&b.project_id, None);
    assert_eq!(
        (&v["name"], &v["pathHint"]),
        (&json!("Renamed"), &Value::Null)
    );
    assert_eq!(update("  ", "").refused(), "the name cannot be empty");
}

#[test]
fn a_recapture_adds_history_and_restore_brings_an_old_snapshot_back() {
    let env = Env::new("cmd-history");
    let a = env.capture("alpha", ALPHA);
    env.capture("alpha", "A2=1\n");
    let v = env.view(&a.project_id, None);
    assert_eq!(v["snapshots"].as_array().unwrap().len(), 2);
    assert_eq!(v["entryCount"], 1, "the latest is the recapture");
    let oldest = &v["snapshots"][1];
    assert_eq!(oldest["entryCount"], 5);

    let old_id = oldest["id"].as_str().unwrap().to_string();
    let id = library::restore_snapshot(env.state(), a.project_id.clone(), old_id).unwrap();
    let v = env.view(&a.project_id, None);
    assert_eq!(v["snapshots"].as_array().unwrap().len(), 3);
    assert_eq!(v["snapshotId"], id.as_str());
    assert_eq!(v["entryCount"], 5);
    assert_eq!(v["via"], "restore");
}

#[test]
fn delete_removes_a_project() {
    let env = Env::new("cmd-delete");
    env.capture("alpha", "A=1\n");
    let b = env.capture("beta", "B=1\n");
    library::delete_project(env.state(), b.project_id.clone()).unwrap();
    assert_eq!(env.projects().len(), 1);
    assert_eq!(env.on_disk().projects.len(), 1);
    let again = library::delete_project(env.state(), b.project_id);
    assert_eq!(again.refused(), "project not found");
}

// ----------------------------------------------------------- protection

#[test]
fn encryption_round_trips_and_keeps_the_backup_in_step() {
    let env = Env::new("cmd-encrypt");
    env.capture("alpha", ALPHA);
    let bak = store::backup_path(&env.store_path());
    let backup_encrypted = || crypto::is_encrypted(&fs::read(&bak).unwrap());

    session::enable_encryption(env.state(), "at-rest-pass-123".into()).unwrap();
    assert_eq!(env.status()["encrypted"], true);
    assert!(
        backup_encrypted(),
        "no plaintext backup survives encrypting"
    );

    session::lock(env.state()).unwrap();
    assert_eq!(env.status()["state"], "locked");
    assert!(library::list_projects(env.state()).is_err());
    let err = session::unlock(env.state(), "wrong-pass".into()).refused();
    assert!(err.contains("wrong passphrase"), "{err}");

    session::unlock(env.state(), "at-rest-pass-123".into()).unwrap();
    assert_eq!(env.status()["state"], "unlocked");
    assert_eq!(env.status()["encrypted"], true);
    assert_eq!(env.project("alpha")["entryCount"], 5, "data survives");

    let change = session::change_passphrase(
        env.state(),
        "at-rest-pass-123".into(),
        "at-rest-pass-456".into(),
    );
    change.unwrap();
    session::lock(env.state()).unwrap();
    session::unlock(env.state(), "at-rest-pass-456".into()).unwrap();

    session::disable_encryption(env.state(), "at-rest-pass-456".into()).unwrap();
    assert_eq!(env.status()["encrypted"], false);
    assert!(!backup_encrypted(), "the backup is plaintext again");
    assert_eq!(env.on_disk().projects.len(), 1);
}

#[test]
fn restore_is_only_for_a_store_that_cannot_be_read() {
    let env = Env::new("cmd-restore");
    env.capture("alpha", "A=1\n");
    env.capture("beta", "B=1\n"); // the backup now holds just alpha
    let err = session::restore_backup(env.state()).refused();
    assert!(err.contains("only for when"), "{err}");

    // Damage the store, then load it the way a restart would.
    fs::write(env.store_path(), b"not a store").unwrap();
    let reload = env.state().with(|inner| {
        inner.session = state::init_session(&inner.store_path);
        Ok(())
    });
    reload.unwrap();
    assert_eq!(env.status()["state"], "corrupt");

    let st = to_json(&session::restore_backup(env.state()).unwrap());
    assert_eq!(st["state"], "unlocked");
    assert_eq!(st["projectCount"], 1);
}

// -------------------------------------------------------- store transfer

/// Give `env` one project, "alpha", with three snapshots (the latest
/// has 5 entries), export an encrypted copy of the store, and stage
/// that copy for import the way the picker would.
fn staged_copy(env: &Env) -> String {
    env.capture("alpha", "A=1\n");
    env.capture("alpha", "B=1\n");
    env.capture("alpha", ALPHA);
    let copy = env.path("copy.store");
    let bytes = export::store_copy_bytes(&env.state(), Some(TRANSPORT)).unwrap();
    fs::write(&copy, bytes).unwrap();
    transfer::stage_import(&env.state(), copy).unwrap()
}

#[test]
fn an_encrypted_store_copy_is_inspected_before_import() {
    let env = Env::new("cmd-inspect");
    let short = export::store_copy_bytes(&env.state(), Some("short"));
    assert!(short.refused().contains("at least 8"));

    let live = transfer::stage_import(&env.state(), env.store_path()).unwrap();
    let err = transfer::inspect_import(env.state(), live.clone(), None).refused();
    assert!(err.contains("already using"), "{err}");

    let token = staged_copy(&env);
    let err = transfer::inspect_import(env.state(), live, None).refused();
    assert!(
        err.contains("no longer pending"),
        "a new pick replaces the old token: {err}"
    );

    let locked = to_json(&transfer::inspect_import(env.state(), token.clone(), None).unwrap());
    assert_eq!(locked["encrypted"], true);
    assert_eq!(locked["unlocked"], false);
    assert_eq!(
        locked["projects"],
        json!([]),
        "nothing is shown before unlocking"
    );

    let wrong = transfer::inspect_import(env.state(), token.clone(), Some("wrong-wrong".into()));
    assert!(wrong.refused().contains("wrong passphrase"));

    let open = transfer::inspect_import(env.state(), token, Some(TRANSPORT.into()));
    let open = to_json(&open.unwrap());
    assert_eq!(open["unlocked"], true);
    assert_eq!(
        open["path"],
        env.path("copy.store").to_string_lossy().as_ref()
    );
    let p = &open["projects"][0];
    assert_eq!(open["projects"].as_array().unwrap().len(), 1);
    assert_eq!(p["name"], "alpha");
    assert_eq!(p["snapshotCount"], 3);
    assert_eq!(p["entryCount"], 5);
    assert_eq!(p["conflictsWith"], "alpha");
}

#[test]
fn import_rename_brings_the_copy_in_beside_the_original() {
    let env = Env::new("cmd-import-rename");
    let token = staged_copy(&env);
    let apply = |pass: Option<&str>, decisions: Value| {
        let pass = pass.map(String::from);
        transfer::apply_import(env.state(), token.clone(), pass, decode(decisions))
    };

    let err = apply(None, json!([])).refused();
    assert!(err.contains("passphrase is needed"), "{err}");
    let err = apply(Some(TRANSPORT), json!([])).refused();
    assert!(
        err.contains("no decision"),
        "a conflict needs a decision: {err}"
    );

    let rename = json!([{ "name": "alpha", "action": "rename", "newName": "alpha (imported)" }]);
    let sum = apply(Some(TRANSPORT), rename).unwrap();
    assert_eq!(sum.renamed, 1);
    assert_eq!(env.projects().len(), 2);
    let imported = env.project("alpha (imported)");
    assert_eq!(imported["snapshotCount"], 3, "history comes along");
    assert_eq!(imported["entryCount"], 5);
    assert_ne!(imported["id"], env.project("alpha")["id"], "a fresh id");
}

#[test]
fn import_replace_takes_the_incoming_version_and_skip_changes_nothing() {
    let env = Env::new("cmd-import-replace");
    let token = staged_copy(&env);
    env.capture("alpha", "DIVERGED=1\n");
    assert_eq!(env.project("alpha")["snapshotCount"], 4);
    let apply = |action: &str| {
        let decisions = decode(json!([{ "name": "alpha", "action": action }]));
        let pass = Some(TRANSPORT.to_string());
        transfer::apply_import(env.state(), token.clone(), pass, decisions).unwrap()
    };

    assert_eq!(apply("replace").replaced, 1);
    assert_eq!(env.projects().len(), 1);
    assert_eq!(
        env.project("alpha")["snapshotCount"],
        3,
        "the incoming history won"
    );

    assert_eq!(apply("skip").skipped, 1);
    assert_eq!(env.projects().len(), 1);
    assert_eq!(env.project("alpha")["snapshotCount"], 3);
}

// ------------------------------------------------------- store location

/// Treat the env's store as one an earlier version moved elsewhere, and
/// return where the default location would be.
fn at_custom_location(env: &Env) -> PathBuf {
    let custom = env.store_path().to_string_lossy().to_string();
    let set = env.state().with(|inner| {
        inner.config.store_path = Some(custom);
        state::save_config(&inner.config_path, &inner.config)
    });
    set.unwrap();
    env.path("data").join("envarsa.store")
}

fn move_to(env: &Env, default: PathBuf) -> Result<String, String> {
    env.state()
        .with(|inner| transfer::move_to_default(inner, default))
}

#[test]
fn a_custom_location_moves_to_the_default_and_leaves_the_old_file() {
    let env = Env::new("cmd-move");
    env.capture("alpha", ALPHA);
    let default = at_custom_location(&env);
    assert_eq!(env.status()["customLocation"], true);
    let before = fs::read(env.store_path()).unwrap();

    let old = move_to(&env, default.clone()).unwrap();
    assert_eq!(old, env.store_path().to_string_lossy().as_ref());
    let st = env.status();
    assert_eq!(st["storePath"], default.to_string_lossy().as_ref());
    assert_eq!(st["customLocation"], false);
    assert_eq!(st["projectCount"], 1, "the session carries on");
    assert_eq!(fs::read(&default).unwrap(), before, "a byte-for-byte copy");
    let config = state::load_config(&env.path("config.json"));
    assert!(
        config.store_path.is_none(),
        "the config no longer points away"
    );

    // The old file stays, and later saves go to the copy only.
    env.capture("beta", BETA);
    assert_eq!(fs::read(env.store_path()).unwrap(), before);
    let moved = store::parse_store(&fs::read(&default).unwrap()).unwrap();
    assert_eq!(moved.projects.len(), 2);
}

#[test]
fn moving_the_store_is_refused_under_the_env_var() {
    let env = Env::new("cmd-move-env");
    let default = at_custom_location(&env);
    let forced = env.state().with(|inner| {
        inner.env_override = true;
        Ok(())
    });
    forced.unwrap();
    assert_eq!(env.status()["customLocation"], false);

    let err = move_to(&env, default.clone()).refused();
    assert!(err.contains("ENVARSA_STORE_PATH"), "{err}");
    assert!(!default.exists());
    assert_eq!(
        env.status()["storePath"],
        env.store_path().to_string_lossy().as_ref()
    );
}

#[test]
fn moving_the_store_never_overwrites_a_file_at_the_default() {
    let env = Env::new("cmd-move-occupied");
    let default = at_custom_location(&env);
    fs::create_dir_all(default.parent().unwrap()).unwrap();
    fs::write(&default, b"a stale library").unwrap();

    let err = move_to(&env, default.clone()).refused();
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(fs::read(&default).unwrap(), b"a stale library");
    assert_eq!(env.status()["customLocation"], true, "nothing switched");
}

// ------------------------------------------------------ write .env.local

/// Stage `name` in the env's folder as the target tab's write.
fn target(env: &Env, name: &str) -> Value {
    to_json(&export::stage_target(&env.state(), env.path(name), None, None).unwrap())
}

fn token(staged: &Value) -> String {
    staged["token"].as_str().unwrap().to_string()
}

fn preview(env: &Env, snap: &CaptureResult, token: &str, merge: bool) -> Value {
    let p = export::preview_write(
        env.state(),
        snap.project_id.clone(),
        snap.snapshot_id.clone(),
        token.into(),
        merge,
    );
    to_json(&p.unwrap())
}

fn write(env: &Env, snap: &CaptureResult, token: &str, merge: bool) -> Result<String, String> {
    export::write_env_local(
        env.state(),
        snap.project_id.clone(),
        snap.snapshot_id.clone(),
        token.into(),
        merge,
    )
}

fn read(env: &Env, name: &str) -> String {
    fs::read_to_string(env.path(name)).unwrap()
}

#[test]
fn a_new_env_local_is_the_reserialized_snapshot() {
    let env = Env::new("cmd-write-new");
    let snap = env.capture("app", WRITE);
    let t = target(&env, ".env.local");
    assert_eq!(t["class"], "writable");
    assert_eq!(t["exists"], false);

    // Merging into a file that doesn't exist yet writes it fresh.
    let p = preview(&env, &snap, &token(&t), true);
    assert_eq!(p["resultEntryCount"], 4);
    assert_eq!(p["blocked"], Value::Null);
    assert_eq!(p["added"], json!([]), "nothing is merged into a new file");
    let path = write(&env, &snap, &token(&t), true).unwrap();
    assert_eq!(path, env.path(".env.local").to_string_lossy());
    assert_eq!(read(&env, ".env.local"), WRITE);
}

#[test]
fn a_replace_overwrites_the_target_with_the_chosen_snapshot() {
    let env = Env::new("cmd-write-replace");
    let snap = env.capture("app", WRITE);
    fs::write(env.path(".env.local"), "LOCAL_ONLY=x\n").unwrap();
    let v2 = env.capture("app", "ONLY=now\n");
    let t = target(&env, ".env.local");
    assert_eq!(t["exists"], true);
    write(&env, &v2, &token(&t), false).unwrap();
    assert_eq!(read(&env, ".env.local"), "ONLY=now\n");
    let _ = snap;
}

#[test]
fn a_merge_keeps_local_only_keys_and_updates_shared_ones() {
    let env = Env::new("cmd-write-merge");
    let snap = env.capture("app", WRITE);
    fs::write(
        env.path(".env.local"),
        "# local\nLOCAL_ONLY=keepme\nPORT=oldport\n",
    )
    .unwrap();
    let t = token(&target(&env, ".env.local"));
    let p = preview(&env, &snap, &t, true);
    assert_eq!(p["kept"], json!(["LOCAL_ONLY"]));
    assert_eq!(p["substituted"], json!(["PORT"]));
    assert_eq!(p["added"], json!(["REGION", "API_KEY", "TOKEN"]));

    write(&env, &snap, &t, true).unwrap();
    let text = read(&env, ".env.local");
    assert!(
        text.starts_with("# local\nLOCAL_ONLY=keepme\nPORT=3000\n"),
        "{text}"
    );
    assert!(!text.contains("oldport"));
    assert!(text.contains("\n# Added by Envarsa\n"));
    assert!(text.contains("API_KEY=real-secret"));
    assert_eq!(
        p["resultEntryCount"],
        envfile::entry_count(&text),
        "the preview describes what was written"
    );
}

#[test]
fn an_example_scaffold_fills_values_and_never_leaks_placeholders() {
    let env = Env::new("cmd-write-example");
    let snap = env.capture("app", WRITE);
    let dir = env.path("tpl");
    fs::create_dir_all(&dir).unwrap();
    let example = dir.join(".env.example");
    let template = "# API config\nAPI_KEY=put-your-key-here\nPORT=8080\nUNUSED=placeholder-value\n";
    fs::write(&example, template).unwrap();

    let staged =
        to_json(&export::stage_example(&env.state(), &example, Some(&example), false).unwrap());
    assert_eq!(staged["exampleName"], ".env.example");
    assert_eq!(
        staged["outPath"],
        dir.join(".env.local").to_string_lossy().as_ref()
    );
    assert_eq!(staged["outClass"], "writable");
    assert_eq!(staged["pickRequired"], false);
    assert_eq!(staged["exampleKeys"], json!(["API_KEY", "PORT", "UNUSED"]));

    // The staged kind decides the fill; `merge` only applies to a target.
    let t = token(&staged);
    let p = preview(&env, &snap, &t, false);
    assert_eq!(p, preview(&env, &snap, &t, true));
    assert_eq!(p["substituted"], json!(["API_KEY", "PORT"]));
    assert_eq!(p["emptied"], json!(["UNUSED"]));
    assert_eq!(p["added"], json!(["REGION", "TOKEN"]));

    write(&env, &snap, &t, false).unwrap();
    let text = read(&env, "tpl/.env.local");
    assert!(
        text.starts_with("# API config\nAPI_KEY=real-secret\nPORT=3000\nUNUSED=\n"),
        "{text}"
    );
    assert!(!text.contains("put-your-key-here") && !text.contains("placeholder-value"));
    assert!(text.contains("# Added by Envarsa") && text.contains("REGION=eu-west-1"));
    assert_eq!(
        fs::read_to_string(&example).unwrap(),
        template,
        "the example is only read"
    );
}

#[test]
fn the_target_and_example_tabs_stage_independently() {
    let env = Env::new("cmd-write-tabs");
    let snap = env.capture("app", WRITE);
    let dir = env.path("scaffold");
    fs::create_dir_all(&dir).unwrap();
    let example = dir.join(".env.example");
    fs::write(&example, "# tpl\nPORT=1\n").unwrap();
    let stage_example = || {
        token(&to_json(
            &export::stage_example(&env.state(), &example, Some(&example), false).unwrap(),
        ))
    };

    // Picking an example keeps the target staged, and the reverse.
    let t = token(&target(&env, ".env.local"));
    let e = stage_example();
    assert_eq!(preview(&env, &snap, &t, false)["blocked"], Value::Null);
    assert_eq!(preview(&env, &snap, &e, false)["blocked"], Value::Null);
    write(&env, &snap, &t, false).unwrap();
    assert_eq!(read(&env, ".env.local"), WRITE);

    let e2 = stage_example();
    let t2 = token(&target(&env, ".env.local"));
    write(&env, &snap, &e2, false).unwrap();
    let text = read(&env, "scaffold/.env.local");
    assert!(text.starts_with("# tpl\nPORT=3000\n"), "{text}");

    // A finished write closes the dialog, so the other staged write goes.
    let err = write(&env, &snap, &t2, false).refused();
    assert!(err.contains("no longer staged"), "{err}");
}

/// The project folder remembered from the snapshot's source decides
/// the default target. In the sandbox it only says where the save
/// dialog opens: nothing is staged until the user picks the file.
#[test]
fn the_sandbox_stages_no_default_target_and_says_where_to_pick() {
    let env = Env::new("cmd-write-sandbox");
    let snap = env.capture("app", WRITE);
    let dir = env.path("app");
    let hint = dir.to_string_lossy().to_string();
    library::update_project(env.state(), snap.project_id.clone(), "app".into(), hint).unwrap();
    let default = |sandboxed| {
        to_json(
            &export::default_target(&env.state(), &snap.project_id, &snap.snapshot_id, sandboxed)
                .unwrap(),
        )
    };

    let staged = default(false);
    assert_eq!(
        staged["path"],
        dir.join(".env.local").to_string_lossy().as_ref()
    );
    assert!(staged["token"].is_string());

    let pick = default(true);
    assert_eq!(
        pick,
        json!({ "dir": dir.to_string_lossy(), "pickRequired": true })
    );

    // The save dialog's pick stages the target like any other.
    let t = target(&env, "app/.env.local");
    write(&env, &snap, &token(&t), false).unwrap();
    assert_eq!(read(&env, "app/.env.local"), WRITE);
}

/// In the sandbox an example can't be written beside itself: its
/// template is staged, and the save dialog's pick re-points it. The
/// name guard runs on the picked path like on any other.
#[test]
fn a_sandboxed_example_is_written_where_the_user_picks() {
    let env = Env::new("cmd-write-sandbox-example");
    let snap = env.capture("app", WRITE);
    let dir = env.path("tpl");
    fs::create_dir_all(&dir).unwrap();
    let example = dir.join(".env.example");
    fs::write(&example, "# tpl\nPORT=1\n").unwrap();

    let staged =
        to_json(&export::stage_example(&env.state(), &example, Some(&example), true).unwrap());
    assert_eq!(staged["pickRequired"], true);
    assert_eq!(staged["outPath"], Value::Null);
    assert_eq!(staged["dir"], dir.to_string_lossy().as_ref());
    assert_eq!(staged["exampleKeys"], json!(["PORT"]));
    let err = write(&env, &snap, &token(&staged), false).refused();
    assert!(err.contains("choose where to write"), "{err}");

    let repoint = |token: &str, name: &str| {
        export::repoint_example(&env.state(), token, env.path(name), None)
    };
    let blocked = to_json(&repoint(&token(&staged), ".env.example").unwrap());
    assert_eq!(blocked["class"], "example");
    assert!(preview(&env, &snap, &token(&blocked), false)["blocked"].is_string());
    assert!(write(&env, &snap, &token(&blocked), false).is_err());
    assert!(
        !env.path(".env.example").exists(),
        "the guard still applies"
    );

    // Each pick replaces the example slot, template and all.
    let err = repoint(&token(&staged), ".env.local").refused();
    assert!(err.contains("no longer staged"), "{err}");
    let picked = to_json(&repoint(&token(&blocked), ".env.local").unwrap());
    write(&env, &snap, &token(&picked), false).unwrap();
    assert!(read(&env, ".env.local").starts_with("# tpl\nPORT=3000\n"));

    // A plain target has no template to re-point.
    let t = target(&env, ".env.local");
    let err = repoint(&token(&t), ".env.local").refused();
    assert!(err.contains("no longer staged"), "{err}");
}

#[test]
fn only_a_local_env_file_is_ever_written() {
    let env = Env::new("cmd-write-guard");
    let snap = env.capture("app", WRITE);
    for (name, class) in [
        (".env.example", "example"),
        (".env.local.bak", "other"),
        ("notes.txt", "other"),
    ] {
        let t = target(&env, name);
        assert_eq!(t["class"], class, "{name}");
        let p = preview(&env, &snap, &token(&t), false);
        assert!(
            p["blocked"].is_string(),
            "{name}: the preview shows the refusal"
        );
        let err = write(&env, &snap, &token(&t), false).refused();
        if class == "example" {
            assert!(err.contains("example"), "{err}");
        }
        assert!(!env.path(name).exists(), "{name} was not written");
    }
}

// ---------------------------------------------------------------- editor

#[test]
fn an_edit_saves_a_new_snapshot_and_keeps_history() {
    let env = Env::new("cmd-edit");
    let snap = env.capture("app", WRITE);
    let lines = library::edit_lines(env.state(), snap.project_id.clone(), None).unwrap();
    let mut edited: Vec<Value> = to_json(&lines)
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["key"] != "TOKEN")
        .cloned()
        .collect();
    for l in &mut edited {
        if l["key"] == "PORT" {
            l["value"] = json!("4000");
        }
    }
    edited.push(
        json!({ "kind": "entry", "key": "NEW_KEY", "value": "added-by-editor", "exported": false }),
    );
    let args = json!({ "projectId": snap.project_id, "lines": edited });
    library::save_edited_snapshot(env.state(), decode(args)).unwrap();

    let after = env.view(&snap.project_id, None);
    assert_eq!(after["snapshots"].as_array().unwrap().len(), 2);
    assert_eq!(after["via"], "edit");
    let reveal = |key: &str| {
        let sid = after["snapshotId"].as_str().unwrap();
        env.reveal(&snap.project_id, sid, idx(line(&after, key)))
    };
    assert_eq!(reveal("PORT"), "4000");
    assert_eq!(reveal("NEW_KEY"), "added-by-editor");
    assert!(lines_have_no(&after, "TOKEN"));
    let old = env.view(&snap.project_id, Some(&snap.snapshot_id));
    assert!(!lines_have_no(&old, "TOKEN"), "the old snapshot is intact");
}

fn lines_have_no(view: &Value, key: &str) -> bool {
    lines(view).iter().all(|l| l["key"] != key)
}

#[test]
fn the_editor_refuses_bad_keys_and_can_build_a_project_by_hand() {
    let env = Env::new("cmd-edit-new");
    let snap = env.capture("app", WRITE);
    let bad = json!({
        "projectId": snap.project_id,
        "lines": [{ "kind": "entry", "key": "A B", "value": "1", "exported": false }],
    });
    let err = library::save_edited_snapshot(env.state(), decode(bad)).refused();
    assert!(err.contains("valid key"), "{err}");
    assert_eq!(env.project("app")["snapshotCount"], 1, "nothing saved");

    // No file in, no file out: straight into the store.
    let args = json!({
        "projectName": "cloud",
        "lines": [
            { "kind": "comment", "text": "cloud-only project" },
            { "kind": "entry", "key": "SERVICE_URL", "value": "https://api.example.com", "exported": false },
            { "kind": "entry", "key": "SERVICE_TOKEN", "value": "tok_123", "exported": false },
        ],
    });
    let r = library::save_edited_snapshot(env.state(), decode(args)).unwrap();
    assert_eq!(r.entry_count, 2);
    assert_eq!(env.project("cloud")["entryCount"], 2);
    let v = env.view(&r.project_id, None);
    assert_eq!(lines(&v)[0]["text"], "# cloud-only project");
    assert_eq!(
        env.reveal(
            &r.project_id,
            &r.snapshot_id,
            idx(line(&v, "SERVICE_TOKEN"))
        ),
        "tok_123"
    );
}
