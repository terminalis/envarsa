//! Parsing of .env text into a line model.
//!
//! A snapshot stores its raw text verbatim — exports and "copy block"
//! always hand back the exact bytes that were captured. Parsing happens
//! on demand and is the single source of truth for what counts as an
//! entry, so the display and the stored bytes can never drift apart.

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Blank,
    Comment(String),
    Entry {
        key: String,
        value: String,
        exported: bool,
    },
    Bad(String),
}

pub fn parse(raw: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = if i == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            lines.push(Line::Blank);
        } else if trimmed.starts_with('#') {
            lines.push(Line::Comment(trimmed.to_string()));
        } else {
            let (body, exported) = match trimmed.strip_prefix("export ") {
                Some(rest) => (rest.trim_start(), true),
                None => (trimmed, false),
            };
            match body.split_once('=') {
                Some((k, v)) => {
                    let key = k.trim();
                    let key_ok = !key.is_empty()
                        && !key
                            .chars()
                            .any(|c| c.is_whitespace() || c == '#' || c == '"' || c == '\'');
                    if key_ok {
                        lines.push(Line::Entry {
                            key: key.to_string(),
                            value: parse_value(v.trim()),
                            exported,
                        });
                    } else {
                        lines.push(Line::Bad(line.to_string()));
                    }
                }
                None => lines.push(Line::Bad(line.to_string())),
            }
        }
    }
    lines
}

/// Effective value of a raw .env value token: surrounding quotes are
/// stripped, double-quoted values get common escapes expanded, and
/// unquoted values are cut at an inline ` #` comment.
fn parse_value(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        let inner = &v[1..v.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    } else if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].to_string()
    } else {
        match v.find(" #") {
            Some(pos) => v[..pos].trim_end().to_string(),
            None => v.to_string(),
        }
    }
}

/// Last-wins effective entries, in order of first appearance.
pub fn effective_entries(lines: &[Line]) -> Vec<(String, String)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in lines {
        if let Line::Entry { key, value, .. } = line {
            if !map.contains_key(key) {
                order.push(key.clone());
            }
            map.insert(key.clone(), value.clone());
        }
    }
    order
        .into_iter()
        .map(|k| {
            let v = map[&k].clone();
            (k, v)
        })
        .collect()
}

/// Number of distinct keys in a raw snapshot.
pub fn entry_count(raw: &str) -> usize {
    effective_entries(&parse(raw)).len()
}

// ----------------------------------------------------- serialization

/// Re-serialize a value to the shortest token that `parse_value` maps
/// back to exactly `v`. Only the parsed value is stored (the raw token
/// is lost), so writing a value back out has to reconstruct its quoting.
pub fn format_value(v: &str) -> String {
    if v.is_empty() {
        return String::new();
    }
    if unquoted_is_safe(v) {
        return v.to_string();
    }
    // Single quotes are literal (no escapes), so they round-trip any
    // value free of `'` and of newlines/tabs.
    if !v.contains('\'') && !v.contains(['\n', '\r', '\t']) {
        return format!("'{v}'");
    }
    // Double quotes with escapes — the only shape that can carry a `'`
    // alongside newlines/tabs.
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Unquoted is faithful only when the parser would hand back `v`
/// unchanged: it trims the token, cuts at an inline ` #`, and treats a
/// matched surrounding quote pair as a quoted value.
fn unquoted_is_safe(v: &str) -> bool {
    if v.trim() != v {
        return false; // leading/trailing whitespace would be trimmed off
    }
    if v.contains(" #") {
        return false; // would be cut as an inline comment
    }
    if v.contains(['\n', '\r', '\t']) {
        return false; // would break the line / not round-trip
    }
    let bytes = v.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return false; // parser would mistake it for a quoted value
        }
    }
    true
}

/// One line, without the trailing newline.
pub fn serialize_line(line: &Line) -> String {
    match line {
        Line::Blank => String::new(),
        Line::Comment(text) => text.clone(),
        Line::Bad(raw) => raw.clone(),
        Line::Entry {
            key,
            value,
            exported,
        } => {
            let prefix = if *exported { "export " } else { "" };
            format!("{prefix}{key}={}", format_value(value))
        }
    }
}

/// Join lines with `\n` and terminate with a single trailing newline.
/// An empty list serializes to the empty string.
pub fn serialize_lines(lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&serialize_line(line));
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------- merge

/// What to do with a target key that the source has no value for.
pub enum AbsentPolicy {
    /// Become `KEY=` (blank) — for an example scaffold, so placeholder
    /// secrets are never copied into the output.
    EmptyOut,
    /// Keep the target's own line — for merging into an existing
    /// `.env.local` whose other keys are already real.
    KeepTarget,
}

/// Merge effective `source` entries into a target's line structure:
/// keep the target's comments, blanks, ordering, and each entry's
/// `export`/casing; substitute values for keys the source has; apply
/// `absent` to target keys the source lacks; then append source-only
/// keys (source order) under one attribution comment.
pub fn merge(target_lines: &[Line], source: &[(String, String)], absent: AbsentPolicy) -> String {
    use std::collections::{HashMap, HashSet};
    let source_map: HashMap<&str, &str> =
        source.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut used: HashSet<&str> = HashSet::new();

    let mut out: Vec<Line> = Vec::with_capacity(target_lines.len() + source.len() + 2);
    for line in target_lines {
        match line {
            Line::Entry {
                key,
                exported,
                ..
            } => {
                if let Some(sv) = source_map.get(key.as_str()) {
                    used.insert(key.as_str());
                    out.push(Line::Entry {
                        key: key.clone(),
                        value: (*sv).to_string(),
                        exported: *exported,
                    });
                } else {
                    match absent {
                        AbsentPolicy::EmptyOut => out.push(Line::Entry {
                            key: key.clone(),
                            value: String::new(),
                            exported: *exported,
                        }),
                        AbsentPolicy::KeepTarget => out.push(line.clone()),
                    }
                }
            }
            other => out.push(other.clone()),
        }
    }

    let extra: Vec<&(String, String)> = source
        .iter()
        .filter(|(k, _)| !used.contains(k.as_str()))
        .collect();
    if !extra.is_empty() {
        // One blank separator unless the target already ended blank/empty.
        if !matches!(out.last(), None | Some(Line::Blank)) {
            out.push(Line::Blank);
        }
        out.push(Line::Comment("# Added by Envarsa".to_string()));
        for (k, v) in extra {
            out.push(Line::Entry {
                key: k.clone(),
                value: v.clone(),
                exported: false,
            });
        }
    }

    serialize_lines(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(lines: &[Line], idx: usize) -> (&str, &str, bool) {
        match &lines[idx] {
            Line::Entry {
                key,
                value,
                exported,
            } => (key.as_str(), value.as_str(), *exported),
            other => panic!("line {idx} is not an entry: {other:?}"),
        }
    }

    #[test]
    fn parses_basic_file() {
        let raw = "# Database\nDATABASE_URL=postgres://localhost/dev\n\nPORT=3000\n";
        let lines = parse(raw);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], Line::Comment("# Database".into()));
        assert_eq!(entry(&lines, 1), ("DATABASE_URL", "postgres://localhost/dev", false));
        assert_eq!(lines[2], Line::Blank);
        assert_eq!(entry(&lines, 3), ("PORT", "3000", false));
    }

    #[test]
    fn strips_bom_and_handles_crlf() {
        let raw = "\u{feff}A=1\r\nB=2\r\n";
        let lines = parse(raw);
        assert_eq!(entry(&lines, 0), ("A", "1", false));
        assert_eq!(entry(&lines, 1), ("B", "2", false));
    }

    #[test]
    fn handles_export_prefix() {
        let lines = parse("export AWS_REGION=eu-west-1");
        assert_eq!(entry(&lines, 0), ("AWS_REGION", "eu-west-1", true));
    }

    #[test]
    fn unquotes_values() {
        let lines = parse("A=\"hello world\"\nB='single $literal'\nC=\"line\\nbreak \\\"q\\\"\"");
        assert_eq!(entry(&lines, 0).1, "hello world");
        assert_eq!(entry(&lines, 1).1, "single $literal");
        assert_eq!(entry(&lines, 2).1, "line\nbreak \"q\"");
    }

    #[test]
    fn cuts_inline_comments_only_when_unquoted() {
        let lines = parse("A=value # note\nB=\"kept # inside\"\nC=url#fragment");
        assert_eq!(entry(&lines, 0).1, "value");
        assert_eq!(entry(&lines, 1).1, "kept # inside");
        assert_eq!(entry(&lines, 2).1, "url#fragment");
    }

    #[test]
    fn empty_value_is_fine() {
        let lines = parse("EMPTY=");
        assert_eq!(entry(&lines, 0), ("EMPTY", "", false));
    }

    #[test]
    fn marks_bad_lines() {
        let lines = parse("not a line\nBAD KEY=1\n=nokey");
        assert!(matches!(lines[0], Line::Bad(_)));
        assert!(matches!(lines[1], Line::Bad(_)));
        assert!(matches!(lines[2], Line::Bad(_)));
    }

    #[test]
    fn effective_entries_last_wins_first_seen_order() {
        let lines = parse("A=1\nB=2\nA=3");
        let eff = effective_entries(&lines);
        assert_eq!(eff, vec![("A".into(), "3".into()), ("B".into(), "2".into())]);
        assert_eq!(entry_count("A=1\nB=2\nA=3"), 2);
    }

    #[test]
    fn format_value_round_trips() {
        for v in [
            "",
            "3000",
            "hello world",
            "a # b",
            " lead",
            "trail ",
            "with\"q",
            "back\\slash",
            "line\nbreak",
            "tab\there",
            "it's",
            "url#frag",
            "'foo'",
            "\"already\"",
        ] {
            let token = format_value(v);
            let lines = parse(&format!("K={token}"));
            let got = match &lines[0] {
                Line::Entry { value, .. } => value.clone(),
                other => panic!("not an entry: {other:?} (token {token:?})"),
            };
            assert_eq!(got, v, "round-trip for {v:?} via token {token:?}");
        }
    }

    #[test]
    fn format_value_is_minimal() {
        assert_eq!(format_value(""), "");
        assert_eq!(format_value("3000"), "3000");
        assert_eq!(format_value("hello world"), "hello world"); // interior space stays unquoted
        assert_eq!(format_value("url#frag"), "url#frag"); // bare # without a leading space
        assert_eq!(format_value("a # b"), "'a # b'");
        assert_eq!(format_value(" lead"), "' lead'");
    }

    #[test]
    fn serialize_line_is_verbatim_for_non_entries_and_honors_export() {
        assert_eq!(serialize_line(&Line::Blank), "");
        assert_eq!(serialize_line(&Line::Comment("# note".into())), "# note");
        assert_eq!(serialize_line(&Line::Bad("Authorization: Bearer x".into())), "Authorization: Bearer x");
        assert_eq!(
            serialize_line(&Line::Entry { key: "R".into(), value: "eu".into(), exported: true }),
            "export R=eu"
        );
    }

    #[test]
    fn serialize_lines_round_trips_a_simple_snapshot() {
        let raw = "# App\nPORT=3000\nexport REGION=eu-west-1\n\n# Extra\nDEBUG=true\n";
        assert_eq!(serialize_lines(&parse(raw)), raw);
    }

    #[test]
    fn merge_empty_out_scaffold_blanks_placeholders() {
        let example = parse("# header\nAPI_KEY=changeme\nPORT=3000\n");
        let source = vec![("API_KEY".to_string(), "real-secret".to_string())];
        let out = merge(&example, &source, AbsentPolicy::EmptyOut);
        assert_eq!(out, "# header\nAPI_KEY=real-secret\nPORT=\n");
        assert!(!out.contains("changeme"), "placeholder must not leak");
        assert!(!out.contains("3000"), "unfilled placeholder must be blanked");
    }

    #[test]
    fn merge_appends_source_only_keys_under_header() {
        let example = parse("# header\nAPI_KEY=x\n");
        let source = vec![
            ("API_KEY".to_string(), "v".to_string()),
            ("EXTRA".to_string(), "y".to_string()),
        ];
        let out = merge(&example, &source, AbsentPolicy::EmptyOut);
        assert_eq!(out, "# header\nAPI_KEY=v\n\n# Added by Envarsa\nEXTRA=y\n");
    }

    #[test]
    fn merge_keep_target_preserves_local_values_and_substitutes() {
        let target = parse("# local\nA=keepme\nB=old\n");
        let source = vec![
            ("B".to_string(), "new".to_string()),
            ("C".to_string(), "added".to_string()),
        ];
        let out = merge(&target, &source, AbsentPolicy::KeepTarget);
        assert_eq!(out, "# local\nA=keepme\nB=new\n\n# Added by Envarsa\nC=added\n");
    }

    #[test]
    fn merge_preserves_export_and_skips_header_when_no_extras() {
        let target = parse("export TOKEN=old\n");
        let source = vec![("TOKEN".to_string(), "new".to_string())];
        assert_eq!(merge(&target, &source, AbsentPolicy::KeepTarget), "export TOKEN=new\n");
    }
}
