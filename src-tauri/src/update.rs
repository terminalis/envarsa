//! The update check — the only code in Envarsa that touches the
//! network, kept in one module so the whole egress surface is a single
//! audit. One hardcoded HTTPS GET to GitHub with an 8s timeout, no
//! redirects, the platform's own TLS stack (schannel on Windows,
//! OpenSSL on Linux); the reply is untrusted input and nothing
//! from it is used unless it parses as a semver version. Nothing
//! downloads, nothing installs, nothing about the library is sent.
//!
//! It runs in exactly two cases: the user clicks "Check for updates",
//! or the user has turned on the automatic check (off by default) —
//! then at most once per 24h, shortly after launch. Builds that update
//! through a store (Microsoft Store, Flatpak) never run it.
//!
//! Invariant: ONE-EGRESS (ARCHITECTURE.md).

use crate::channel;
use crate::state::{self, AppState};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Opened in the browser when an update is found. A compile-time
/// constant — nothing fetched ever becomes a link.
const RELEASES_PAGE_URL: &str = "https://github.com/terminalis/envarsa/releases/latest";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/terminalis/envarsa/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(8);
/// The release JSON is ~10-30 KB; cap reads hard anyway.
const MAX_BODY_BYTES: u64 = 256 * 1024;
const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;

/// The version of this build: Cargo.toml's, which is also what Tauri
/// reports, since tauri.conf.json carries no version of its own.
pub fn running_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo.toml has a semver version")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                // The OS does the handshake (schannel on Windows,
                // OpenSSL on Linux) and verifies against the system
                // cert store (so enterprise roots keep working).
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .https_only(true)
        // GitHub's API rejects requests without a User-Agent.
        .user_agent(format!("envarsa/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent()
}

/// Blocking; callers run it on a worker thread.
fn fetch_latest_version() -> Result<semver::Version, String> {
    let mut resp = agent()
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(403) | ureq::Error::StatusCode(429) => {
                "GitHub is rate-limiting this network — try again later".to_string()
            }
            ureq::Error::StatusCode(404) => {
                "GitHub lists no published release to compare against".to_string()
            }
            ureq::Error::StatusCode(code) => format!("GitHub answered with HTTP {code}"),
            ureq::Error::Timeout(_) => "GitHub did not answer in time".to_string(),
            other => format!("could not reach GitHub: {other}"),
        })?;
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_string()
        .map_err(|e| format!("could not read GitHub's answer: {e}"))?;
    parse_latest_response(&body)
}

#[derive(serde::Deserialize)]
struct LatestRelease {
    // Every other field in the response is ignored by serde.
    tag_name: String,
}

fn parse_latest_response(body: &str) -> Result<semver::Version, String> {
    let release: LatestRelease = serde_json::from_str(body)
        .map_err(|_| "GitHub's answer was not in the expected shape".to_string())?;
    parse_tag(&release.tag_name)
}

/// The trust boundary for the wire: release tags are `vX.Y.Z` (the
/// release workflow enforces tag == app version), and nothing from the
/// response crosses this function unparsed.
pub(crate) fn parse_tag(tag: &str) -> Result<semver::Version, String> {
    let t = tag.trim();
    if t.is_empty() || t.len() > 64 {
        return Err("GitHub answered with an unusable release tag".into());
    }
    let bare = t.strip_prefix(['v', 'V']).unwrap_or(t);
    semver::Version::parse(bare)
        .map_err(|_| "GitHub answered with an unusable release tag".to_string())
}

/// The automatic path: spawned once at startup, does nothing unless the
/// user opted in and a check is due. Failures are silent by design —
/// the manual button is the loud path.
pub fn maybe_spawn_auto_check(app: AppHandle) {
    // Store and Flatpak builds update through their store; the in-app
    // check points at GitHub, so it must never fire there — even if a user
    // flipped the opt-in toggle (e.g. in a config carried over from a
    // direct download).
    if channel::current().updates_externally() {
        return;
    }
    std::thread::spawn(move || {
        // Off the boot path; the window paints first.
        std::thread::sleep(Duration::from_secs(3));

        let due = app.state::<AppState>().with(|inner| {
            if !inner.config.auto_update_check {
                return Ok(false);
            }
            let now = chrono::Utc::now().timestamp();
            let due = match inner.config.last_update_check {
                None => true,
                // `last > now` self-heals a clock that jumped backwards.
                Some(last) => now - last >= CHECK_INTERVAL_SECS || last > now,
            };
            if due {
                // Stamp before fetching, so a failing network can never
                // retry-storm across relaunches.
                inner.config.last_update_check = Some(now);
                let _ = state::save_config(&inner.config_path, &inner.config);
            }
            Ok(due)
        });
        if due != Ok(true) {
            return;
        }

        let Ok(latest) = fetch_latest_version() else {
            return;
        };
        if record_check(&app, &latest) {
            let _ = app.emit("update-available", latest.to_string());
        }
    });
}

/// Record a successful check, manual or automatic: stamp the time, keep
/// `latest` only while it is newer than the running version, and persist
/// both. Best effort — a config-write failure must not eat a good answer.
/// Returns whether `latest` is newer.
fn record_check(app: &AppHandle, latest: &semver::Version) -> bool {
    let now = chrono::Utc::now().timestamp();
    let current = &running_version();
    let newer = latest > current;
    let _ = app.state::<AppState>().with(|inner| {
        apply_check(&mut inner.config, latest, current, now);
        state::save_config(&inner.config_path, &inner.config)
    });
    newer
}

fn apply_check(
    config: &mut state::Config,
    latest: &semver::Version,
    current: &semver::Version,
    now: i64,
) {
    config.last_update_check = Some(now);
    config.available_version = (latest > current).then(|| latest.to_string());
}

// -------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub latest: String,
    pub update_available: bool,
}

/// The manual "Check for updates" button — the loud path: failures come
/// back as errors for the settings modal to show inline. A manual check
/// legitimately postpones the next automatic one.
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    if channel::current().updates_externally() {
        return Err(
            "This build updates through the Microsoft Store or Flathub, so the in-app check is off."
                .into(),
        );
    }
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_version)
        .await
        .map_err(|e| format!("update check failed: {e}"))??;
    let update_available = record_check(&app, &latest);
    Ok(UpdateCheckResult {
        latest: latest.to_string(),
        update_available,
    })
}

#[tauri::command]
pub fn set_auto_update_check(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
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
pub fn open_releases_page(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(RELEASES_PAGE_URL, None::<&str>)
        .map_err(|e| format!("could not open the releases page: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_parse_with_and_without_prefix() {
        assert_eq!(parse_tag("v0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(parse_tag("0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(parse_tag("V0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(parse_tag(" v0.2.0 ").unwrap().to_string(), "0.2.0");
        assert_eq!(parse_tag("v0.2.0-rc.1").unwrap().to_string(), "0.2.0-rc.1");
    }

    #[test]
    fn junk_tags_are_refused() {
        assert!(parse_tag("").is_err());
        assert!(parse_tag("nightly").is_err());
        assert!(parse_tag("v1.2").is_err());
        assert!(parse_tag("1.2.3.4").is_err());
        assert!(
            parse_tag(&"v1.0.0".repeat(20)).is_err(),
            "over-long tags refused"
        );
    }

    #[test]
    fn release_json_yields_the_tag() {
        let body = r#"{
            "url": "https://api.github.com/repos/terminalis/envarsa/releases/1",
            "tag_name": "v0.2.0",
            "name": "Envarsa 0.2.0",
            "body": "release notes are never parsed or shown"
        }"#;
        assert_eq!(parse_latest_response(body).unwrap().to_string(), "0.2.0");
    }

    #[test]
    fn bad_responses_are_refused() {
        assert!(parse_latest_response("not json").is_err());
        assert!(parse_latest_response(r#"{"name": "no tag here"}"#).is_err());
        assert!(parse_latest_response(r#"{"tag_name": "garbage"}"#).is_err());
    }

    /// Manual probe (`cargo test live_probe -- --ignored`): proves the
    /// platform TLS path and the request shape against the real GitHub
    /// API. Passes whether or not a release is published — it only
    /// fails on transport-level errors (DNS, TLS, timeout), which the
    /// error text distinguishes from HTTP statuses.
    #[test]
    #[ignore = "hits the network — run explicitly"]
    fn live_probe_reaches_github() {
        match fetch_latest_version() {
            Ok(v) => println!("latest published release: {v}"),
            Err(e) => {
                println!("no usable release ({e})");
                assert!(
                    !e.starts_with("could not reach GitHub")
                        && e != "GitHub did not answer in time",
                    "transport-level failure: {e}"
                );
            }
        }
    }

    #[test]
    fn a_recorded_check_keeps_only_a_newer_version() {
        let current = semver::Version::parse("1.1.0").unwrap();
        let mut c = state::Config::default();

        apply_check(&mut c, &parse_tag("v1.2.0").unwrap(), &current, 100);
        assert_eq!(c.last_update_check, Some(100));
        assert_eq!(c.available_version.as_deref(), Some("1.2.0"));

        // Once up to date, the stale "available" answer is cleared.
        apply_check(&mut c, &parse_tag("v1.1.0").unwrap(), &current, 200);
        assert_eq!(c.last_update_check, Some(200));
        assert_eq!(c.available_version, None);
    }

    #[test]
    fn version_ordering_matches_semver() {
        let current = semver::Version::parse("0.1.0").unwrap();
        assert!(parse_tag("v0.2.0").unwrap() > current);
        assert!(parse_tag("v0.1.0").unwrap() == current);
        assert!(parse_tag("v0.0.9").unwrap() < current);
        // A prerelease of the next version is still newer than current,
        // but older than its own release.
        let rc = parse_tag("v0.2.0-rc.1").unwrap();
        assert!(rc > current);
        assert!(rc < parse_tag("v0.2.0").unwrap());
    }
}
