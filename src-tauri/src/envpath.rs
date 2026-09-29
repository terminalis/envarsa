//! Classifies a target filename for the one place Envarsa writes into a
//! project tree: a `.env*.local` file.
//!
//! The rule is enforced here, in the core, on the final resolved path —
//! never in the webview — so a write can only land on a gitignored
//! `.env*.local`, and never on a git-committed example file where a
//! secret would leak into version control.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameClass {
    /// `.env.local`, `.env.development.local`, … — the only writable shape.
    WritableLocal,
    /// `.env.example`/`.sample`/`.template`/`.dist` — always refused.
    ExampleFamily,
    /// Anything else (bare `.env`, `.env.local.bak`, `notes.txt`, …).
    Other,
}

// Not yet called from commands.rs.
#[allow(dead_code)]
impl NameClass {
    /// "writable" | "example" | "other" — for the UI badge.
    pub fn as_str(self) -> &'static str {
        match self {
            NameClass::WritableLocal => "writable",
            NameClass::ExampleFamily => "example",
            NameClass::Other => "other",
        }
    }

    /// Why a write to this class is refused; `None` when it's writable.
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            NameClass::WritableLocal => None,
            NameClass::ExampleFamily => Some(
                "refusing to write into an example file — .env.example/.sample/.template/.dist are \
                 committed to git, so secrets would leak. Write to a .env.local instead.",
            ),
            NameClass::Other => Some(
                "Envarsa only writes to the .env*.local family (.env.local, .env.development.local, …), \
                 which is gitignored.",
            ),
        }
    }
}

/// Classify by the final path segment alone, case-insensitively. Example
/// markers win over everything, so `.env.example.local` is refused too.
pub fn classify_name(path: &Path) -> NameClass {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
        return NameClass::Other;
    };
    let segments: Vec<&str> = name.split('.').collect();
    if segments
        .iter()
        .any(|s| matches!(*s, "example" | "sample" | "template" | "dist"))
    {
        return NameClass::ExampleFamily;
    }
    if segments.iter().any(|s| *s == "env") && segments.last() == Some(&"local") {
        return NameClass::WritableLocal;
    }
    NameClass::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> NameClass {
        classify_name(Path::new(s))
    }

    #[test]
    fn writable_local_family() {
        for n in [
            ".env.local",
            ".env.development.local",
            ".env.production.local",
            ".env.test.local",
            ".env.staging.local",
            ".ENV.Local",
            "env.local",
            "myapp.env.local",
            "/home/u/app/.env.local",
        ] {
            assert_eq!(c(n), NameClass::WritableLocal, "{n}");
        }
    }

    #[test]
    fn example_family_is_blocked() {
        for n in [
            ".env.example",
            ".env.sample",
            ".env.template",
            ".env.dist",
            ".env.production.sample",
            "config.env.template",
            // The trap: an example marker wins over the .local suffix.
            ".env.example.local",
            "/home/u/app/.env.example",
        ] {
            assert_eq!(c(n), NameClass::ExampleFamily, "{n}");
        }
    }

    #[test]
    fn everything_else_is_other() {
        for n in [
            ".env",
            ".env.local.bak",
            "local",
            "notes.txt",
            "myenv.local", // no exact "env" segment
            ".env.local.", // trailing dot — last segment isn't "local"
            "",
        ] {
            assert_eq!(c(n), NameClass::Other, "{n}");
        }
    }

    #[test]
    fn only_writable_local_is_allowed() {
        assert_eq!(c(".env.local").as_str(), "writable");
        assert_eq!(c(".env.example").as_str(), "example");
        assert_eq!(c(".env").as_str(), "other");
        assert_eq!(c(".env.local").refusal(), None);
        assert!(c(".env.example")
            .refusal()
            .is_some_and(|r| r.contains("example file")));
        assert!(c(".env")
            .refusal()
            .is_some_and(|r| r.contains(".env*.local")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_backslash_paths() {
        assert_eq!(c("C:\\dev\\app\\.env.local"), NameClass::WritableLocal);
        assert_eq!(c("C:\\dev\\app\\.env.example"), NameClass::ExampleFamily);
    }
}
