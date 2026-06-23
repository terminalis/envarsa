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
//! then at most once per 24h, shortly after launch.

use crate::state::{self, AppState};
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Opened in the browser when an update is found. A compile-time
/// constant — nothing fetched ever becomes a link.
pub const RELEASES_PAGE_URL: &str = "https://github.com/terminalis/envarsa/releases/latest";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/terminalis/envarsa/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(8);
/// The release JSON is ~10-30 KB; cap reads hard anyway.
const MAX_BODY_BYTES: u64 = 256 * 1024;
const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;

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
pub fn fetch_latest_version() -> Result<semver::Version, String> {
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

/// True when Envarsa is running as its packaged (MSIX / Microsoft Store)
/// build. Store users are updated through the Store, so the in-app update
/// check — which points at GitHub releases — must be suppressed in that
/// case. Detected via the Win32 `GetCurrentPackageFullName` (the Win32
/// face of `Package.Current`): it answers `APPMODEL_ERROR_NO_PACKAGE` for
/// an unpackaged process, and any other status (here `ERROR_INSUFFICIENT_BUFFER`,
/// since the query buffer is empty) means a package identity exists.
/// Always false off Windows.
#[cfg(windows)]
pub fn is_packaged() -> bool {
    use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len: u32 = 0;
    // SAFETY: the documented "query" form — a length pointer with no
    // output buffer. The call writes only `len` and returns a status code.
    let rc = unsafe { GetCurrentPackageFullName(&mut len, None) };
    rc != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(not(windows))]
pub fn is_packaged() -> bool {
    false
}

/// The automatic path: spawned once at startup, does nothing unless the
/// user opted in and a check is due. Failures are silent by design —
/// the manual button is the loud path.
pub fn maybe_spawn_auto_check(app: tauri::AppHandle) {
    // The selftest must stay offline and deterministic — and it reads
    // the user's real config.json, where the toggle may be on.
    if std::env::var("ENVARSA_SELFTEST").is_ok() {
        return;
    }
    // Store builds update through the Store; the in-app check points at
    // GitHub, so it must never fire when packaged — even if a user flipped
    // the opt-in toggle (e.g. in a config carried over from a non-Store
    // build).
    if is_packaged() {
        return;
    }
    std::thread::spawn(move || {
        // Off the boot path; the window paints first.
        std::thread::sleep(Duration::from_secs(3));

        let due = {
            let state = app.state::<AppState>();
            let mut guard = match state.0.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(inner) = guard.as_mut() else { return };
            if !inner.config.auto_update_check {
                return;
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
            due
        };
        if !due {
            return;
        }

        let Ok(latest) = fetch_latest_version() else { return };
        let newer = latest > app.package_info().version;

        {
            let state = app.state::<AppState>();
            let mut guard = match state.0.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Some(inner) = guard.as_mut() {
                inner.config.available_version = newer.then(|| latest.to_string());
                let _ = state::save_config(&inner.config_path, &inner.config);
            }
        }
        if newer {
            let _ = app.emit("update-available", latest.to_string());
        }
    });
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
        assert!(parse_tag(&"v1.0.0".repeat(20)).is_err(), "over-long tags refused");
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
