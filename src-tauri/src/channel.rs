//! How this copy of Envarsa was installed: downloaded from GitHub,
//! from the Microsoft Store, or as a Flatpak. Detected once and cached;
//! the rest of the core asks this module instead of probing the system
//! itself.

use serde::Serialize;
use std::sync::OnceLock;

/// Serialized for the UI as `"direct"`, `"microsoft-store"` or `"flatpak"`.
/// A Mac App Store variant would answer yes to both questions below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    /// The installer from GitHub Releases, or a development build.
    Direct,
    #[cfg_attr(not(windows), allow(dead_code))]
    MicrosoftStore,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Flatpak,
}

impl Channel {
    /// Updates arrive through a store, so the in-app update check
    /// (which points at GitHub releases) stays off.
    pub fn updates_externally(self) -> bool {
        matches!(self, Channel::MicrosoftStore | Channel::Flatpak)
    }

    /// Files outside the app's own folders are reachable only through
    /// portals. MSIX desktop apps are full-trust, so only the Flatpak.
    #[allow(dead_code)] // no caller until the Flatpak's file handling
    pub fn sandboxed(self) -> bool {
        matches!(self, Channel::Flatpak)
    }
}

/// The channel of the running process.
pub fn current() -> Channel {
    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    *CHANNEL.get_or_init(detect)
}

fn detect() -> Channel {
    #[cfg(windows)]
    if has_package_identity() {
        return Channel::MicrosoftStore;
    }
    #[cfg(target_os = "linux")]
    if in_flatpak(std::path::Path::new("/.flatpak-info")) {
        return Channel::Flatpak;
    }
    Channel::Direct
}

/// True when Envarsa is running as its packaged (MSIX / Microsoft Store)
/// build. Store users are updated through the Store, so the in-app update
/// check — which points at GitHub releases — must be suppressed in that
/// case. Detected via the Win32 `GetCurrentPackageFullName` (the Win32
/// face of `Package.Current`): it answers `APPMODEL_ERROR_NO_PACKAGE` for
/// an unpackaged process, and any other status (here `ERROR_INSUFFICIENT_BUFFER`,
/// since the query buffer is empty) means a package identity exists.
#[cfg(windows)]
fn has_package_identity() -> bool {
    use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len: u32 = 0;
    // SAFETY: the documented "query" form — a length pointer with no
    // output buffer. The call writes only `len` and returns a status code.
    let rc = unsafe { GetCurrentPackageFullName(&mut len, None) };
    rc != APPMODEL_ERROR_NO_PACKAGE
}

/// Flatpak puts `/.flatpak-info` at the root of every sandbox it starts.
#[cfg(target_os = "linux")]
fn in_flatpak(info: &std::path::Path) -> bool {
    info.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_serialize_for_the_ui() {
        let names = [Channel::Direct, Channel::MicrosoftStore, Channel::Flatpak]
            .map(|c| serde_json::to_value(c).unwrap());
        assert_eq!(names, ["direct", "microsoft-store", "flatpak"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_flatpak_is_recognised_by_its_info_file() {
        let dir = crate::store::fixtures::tmp_dir("flatpak-info");
        let info = dir.join(".flatpak-info");
        assert!(!in_flatpak(&info));
        std::fs::write(&info, "[Application]\nname=dev.envarsa.Envarsa\n").unwrap();
        assert!(in_flatpak(&info));
    }
}
