//! Stable vault-relative identities and parsed-note values.
use serde::{Deserialize, Serialize};
use std::{ops::Range, sync::Arc, time::SystemTime};
/// A normalized vault-relative note path without its Markdown suffix.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NoteId(Arc<str>);
/// An invalid or escaping note identity.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid vault-relative note id: {0}")]
pub struct IdError(pub String);
impl NoteId {
    /// Normalize separators and dot components, rejecting absolute or escaping paths.
    pub fn new(value: &str) -> Result<Self, IdError> {
        let value = value.replace('\\', "/");
        if value.starts_with('/') || value.contains(['\0', ':']) {
            return Err(IdError(value));
        }
        let mut parts = Vec::new();
        for part in value.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(IdError(value));
                    }
                }
                p => parts.push(p),
            }
        }
        let mut normalized = parts.join("/");
        if normalized.to_ascii_lowercase().ends_with(".md") {
            normalized.truncate(normalized.len() - 3);
        }
        if normalized.is_empty() {
            return Err(IdError(value));
        }
        Ok(Self(normalized.into()))
    }
    /// Borrow the normalized identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Last path component.
    pub fn stem(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or_default()
    }
    /// Parent relative folder, empty for the root.
    pub fn folder(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(p, _)| p)
    }
    /// Relative filename with the Markdown suffix.
    pub fn filename(&self) -> String {
        format!("{}.md", self.0)
    }
}
impl TryFrom<String> for NoteId {
    type Error = IdError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(&v)
    }
}
impl From<NoteId> for String {
    fn from(v: NoteId) -> Self {
        v.0.to_string()
    }
}
impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
/// A case-folded, nested tag without its hash prefix.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tag(pub String);
impl Tag {
    /// Fold a tag while preserving nesting.
    pub fn new(s: &str) -> Self {
        Self(s.trim().trim_start_matches('#').to_lowercase())
    }
}
/// Slug used for heading navigation.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            out.push(c);
            gap = false;
        } else {
            gap = true;
        }
    }
    out
}
/// A source heading, with a one-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Markdown heading level.
    pub level: u8,
    /// Visible text.
    pub text: String,
    /// Normalized navigation key.
    pub slug: String,
    /// One-based source line.
    pub line: u32,
}
/// A link after resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Note and optional navigation fragment.
    Note {
        /// Target note.
        id: NoteId,
        /// Heading slug, even when missing.
        heading: Option<String>,
        /// Block identifier.
        block: Option<String>,
    },
    /// No note currently matches.
    Unresolved(String),
    /// URL (launch policy is checked separately).
    External(String),
    /// Vault-relative attachment path.
    Attachment(String),
}
/// Source link syntax, needed for byte-preserving rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Obsidian brackets.
    Wiki,
    /// `CommonMark` destination.
    Markdown,
}
/// A parsed outgoing link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Destination as written, without alias.
    pub raw: String,
    /// Resolved destination.
    pub target: LinkTarget,
    /// Display alias.
    pub alias: Option<String>,
    /// Image or note transclusion.
    pub embed: bool,
    /// One-based source line.
    pub line: u32,
    /// One-based byte column.
    pub col: u32,
    /// Destination-only byte span for rewrites.
    pub byte_range: Range<usize>,
    /// Full link source span.
    pub span: Range<usize>,
    /// Source notation.
    pub kind: LinkKind,
}
/// Metadata extracted without retaining the note body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteMeta {
    /// Vault identity.
    pub id: NoteId,
    /// Frontmatter title or file stem.
    pub title: String,
    /// Frontmatter aliases.
    pub aliases: Vec<String>,
    /// Sorted, deduplicated tags.
    pub tags: Vec<Tag>,
    /// Source headings.
    pub headings: Vec<Heading>,
    /// Outgoing links.
    pub links: Vec<Link>,
    /// Block identifiers and one-based source lines.
    pub blocks: Vec<(String, u32)>,
    /// Frontmatter byte range, including delimiters.
    pub frontmatter: Option<Range<usize>>,
    /// Disk modification timestamp.
    pub mtime: SystemTime,
    /// Original byte size.
    pub size: u64,
}
/// A byte edit applied from highest offset to lowest within each note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Source note.
    pub note: NoteId,
    /// Destination span.
    pub byte_range: Range<usize>,
    /// Replacement destination.
    pub replacement: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities() {
        assert_eq!(NoteId::new("a\\b/../c.md").unwrap().as_str(), "a/c");
        for s in ["../x", "/a", "a/../../x", "", "C:/x"] {
            assert!(NoteId::new(s).is_err());
        }
        assert_eq!(slug(" HéLLo,  世界! "), "héllo-世界");
        assert_eq!(Tag::new("#Hello/WORLD").0, "hello/world");
    }
}
