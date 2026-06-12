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
}
