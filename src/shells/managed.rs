//! The per-shell file of aliases and functions that the sidebar manages.
//!
//! It lives in the shell's own config location ([`path_for`]) but is
//! only ever loaded by Terminaal (see `launch`), so it can't affect other
//! terminals. The file itself is the source of truth: each entry is a
//! block between marker comments, and whatever sits between the markers
//! is ordinary shell code that can be edited by hand as well. Anything
//! outside the markers is dropped on the next save -- the header says so.

use std::io;
use std::path::PathBuf;

use super::ShellKind;
use crate::i18n::t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Alias,
    Function,
}

impl EntryKind {
    pub fn label(self) -> String {
        match self {
            Self::Alias => t!("managed-alias"),
            Self::Function => t!("managed-function"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub name: String,
    /// The alias's command line, or the function's body (unindented,
    /// possibly several lines).
    pub value: String,
}

const MARK_ALIAS: &str = "# terminaal:alias ";
const MARK_FUNCTION: &str = "# terminaal:function ";
const MARK_END: &str = "# terminaal:end";

/// `None` for shells Terminaal can't manage.
pub fn path_for(kind: ShellKind) -> Option<PathBuf> {
    match kind {
        ShellKind::Fish => Some(super::xdg_config_home()?.join("fish/terminaal.fish")),
        ShellKind::Bash => Some(super::home()?.join(".bash_terminaal")),
        ShellKind::Zsh => Some(super::zdotdir()?.join(".zsh_terminaal")),
        ShellKind::Other => None,
    }
}

/// A missing file just means nothing has been defined yet.
pub fn load(kind: ShellKind) -> io::Result<Vec<Entry>> {
    let Some(path) = path_for(kind) else { return Ok(Vec::new()) };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(parse(kind, &text)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

pub fn save(kind: ShellKind, entries: &[Entry]) -> io::Result<()> {
    let path = path_for(kind).ok_or_else(|| io::Error::other(t!("managed-unsupported")))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write-then-rename, so a crash mid-write can't leave a truncated
    // file behind for the next shell to choke on.
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    std::fs::write(&tmp, render(kind, entries))?;
    std::fs::rename(&tmp, &path)
}

/// Rejects names that would need quoting or could be mistaken for an
/// option -- the shells disagree on what else is allowed.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(t!("common-name-missing"));
    }
    if name.starts_with('-') || !name.chars().all(|c| c.is_ascii_alphanumeric() || "_.:+-".contains(c)) {
        return Err(t!("managed-invalid-name"));
    }
    Ok(())
}

pub fn render(kind: ShellKind, entries: &[Entry]) -> String {
    // The header's language is whatever the UI had when last saved.
    let mut out = t!("managed-header");
    out.push('\n');
    for entry in entries {
        out.push('\n');
        out.push_str(match entry.kind {
            EntryKind::Alias => MARK_ALIAS,
            EntryKind::Function => MARK_FUNCTION,
        });
        out.push_str(&entry.name);
        out.push('\n');
        match entry.kind {
            EntryKind::Alias => {
                let line = match kind {
                    ShellKind::Fish => format!("alias {} {}", entry.name, quote_fish(&entry.value)),
                    _ => format!("alias {}={}", entry.name, quote_posix(&entry.value)),
                };
                out.push_str(&line);
                out.push('\n');
            }
            EntryKind::Function => {
                let (open, close) = match kind {
                    ShellKind::Fish => (format!("function {}", entry.name), "end"),
                    _ => (format!("{}() {{", entry.name), "}"),
                };
                out.push_str(&open);
                out.push('\n');
                for line in entry.value.lines() {
                    if !line.is_empty() {
                        out.push_str("    ");
                        out.push_str(line);
                    }
                    out.push('\n');
                }
                out.push_str(close);
                out.push('\n');
            }
        }
        out.push_str(MARK_END);
        out.push('\n');
    }
    out
}

pub fn parse(kind: ShellKind, text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let (entry_kind, name) = if let Some(name) = line.strip_prefix(MARK_ALIAS) {
            (EntryKind::Alias, name)
        } else if let Some(name) = line.strip_prefix(MARK_FUNCTION) {
            (EntryKind::Function, name)
        } else {
            continue;
        };
        let name = name.trim().to_string();
        let block: Vec<&str> = lines.by_ref().take_while(|l| l.trim_end() != MARK_END).collect();
        let value = match entry_kind {
            EntryKind::Alias => parse_alias(kind, &name, &block),
            EntryKind::Function => parse_function(kind, &block),
        };
        entries.push(Entry { kind: entry_kind, name, value });
    }
    entries
}

/// Falls back to the raw line if it isn't the `alias` form we write,
/// so a hand edit is shown (and kept) rather than lost.
fn parse_alias(kind: ShellKind, name: &str, block: &[&str]) -> String {
    let line = block.iter().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("");
    let Some(rest) = line.strip_prefix("alias ").map(str::trim_start).and_then(|r| r.strip_prefix(name)) else {
        return line.to_string();
    };
    match kind {
        // fish accepts both `alias name value` and `alias name=value`.
        ShellKind::Fish => unquote(rest.strip_prefix('=').unwrap_or(rest), true),
        _ => rest.strip_prefix('=').map_or_else(|| line.to_string(), |value| unquote(value, false)),
    }
}

fn parse_function(kind: ShellKind, block: &[&str]) -> String {
    let mut lines = block;
    while let [first, rest @ ..] = lines
        && first.trim().is_empty()
    {
        lines = rest;
    }
    while let [rest @ .., last] = lines
        && last.trim().is_empty()
    {
        lines = rest;
    }
    let close = if kind == ShellKind::Fish { "end" } else { "}" };
    if let [first, inner @ .., last] = lines
        && last.trim() == close
        && match kind {
            ShellKind::Fish => first.trim_start().starts_with("function "),
            _ => first.trim_end().ends_with('{'),
        }
    {
        lines = inner;
    }
    lines
        .iter()
        .map(|&line| line.strip_prefix("    ").or_else(|| line.strip_prefix('\t')).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// POSIX single quotes: nothing inside is special, so a literal `'` has
/// to close the quote, add an escaped `\'` and reopen: `'\''`.
pub(super) fn quote_posix(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// fish's single quotes do honour `\\` and `\'`.
pub(super) fn quote_fish(s: &str) -> String {
    format!("'{}'", s.replace('\\', r"\\").replace('\'', r"\'"))
}

/// Undoes either quoting above, plus double quotes and bare backslash
/// escapes for hand-written lines. `fish` selects fish's single-quote
/// rules.
fn unquote(s: &str, fish: bool) -> String {
    let mut out = String::new();
    let mut chars = s.trim().chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                while let Some(c) = chars.next() {
                    match c {
                        '\'' => break,
                        '\\' if fish => match chars.next() {
                            Some(n @ ('\\' | '\'')) => out.push(n),
                            Some(n) => out.extend(['\\', n]),
                            None => out.push('\\'),
                        },
                        c => out.push(c),
                    }
                }
            }
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(n @ ('"' | '\\' | '$' | '`')) => out.push(n),
                            Some(n) => out.extend(['\\', n]),
                            None => out.push('\\'),
                        },
                        c => out.push(c),
                    }
                }
            }
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Entry> {
        vec![
            Entry { kind: EntryKind::Alias, name: "ll".into(), value: "ls -la".into() },
            Entry { kind: EntryKind::Alias, name: "q".into(), value: r#"echo 'it''s' "x" \ $HOME"#.into() },
            Entry {
                kind: EntryKind::Function,
                name: "mkcd".into(),
                value: "mkdir -p \"$1\"\n\nif true\n    cd \"$1\"\nend".into(),
            },
        ]
    }

    #[test]
    fn round_trips_through_the_file_format() {
        for kind in [ShellKind::Fish, ShellKind::Bash, ShellKind::Zsh] {
            assert_eq!(parse(kind, &render(kind, &sample())), sample(), "{kind:?}");
        }
    }

    #[test]
    fn renders_shell_specific_syntax() {
        let fish = render(ShellKind::Fish, &sample());
        assert!(fish.contains("alias ll 'ls -la'\n"));
        assert!(fish.contains("function mkcd\n    mkdir -p \"$1\"\n\n    if true\n        cd \"$1\"\n    end\nend\n"));
        let bash = render(ShellKind::Bash, &sample());
        assert!(bash.contains(r#"alias q='echo '\''it'\'''\''s'\'' "x" \ $HOME'"#));
        assert!(bash.contains("mkcd() {\n"));
    }

    #[test]
    fn parses_hand_edited_blocks() {
        let text = "stray line\n# terminaal:alias g\nalias g=\"git status\"\n# terminaal:end\n\
                    # terminaal:function hi\nhi() { echo hi; }\n# terminaal:end\n";
        let entries = parse(ShellKind::Bash, text);
        assert_eq!(entries[0].value, "git status");
        // Doesn't match the multi-line layout we write, so kept verbatim.
        assert_eq!(entries[1].value, "hi() { echo hi; }");
    }

    #[test]
    fn validates_names() {
        assert!(validate_name("git-log.v2").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("-x").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("a'b").is_err());
    }
}
