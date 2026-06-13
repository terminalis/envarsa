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

    #[cfg(windows)]
    #[test]
    fn windows_backslash_paths() {
        assert_eq!(c("C:\\dev\\app\\.env.local"), NameClass::WritableLocal);
        assert_eq!(c("C:\\dev\\app\\.env.example"), NameClass::ExampleFamily);
    }
}
