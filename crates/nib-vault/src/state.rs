//! Profile-specific XDG paths, tolerant session storage and crash diagnostics.
use crate::{Guard, VaultError, fsx};
use nib_core::settings::{Config, DEFAULT_TOML, Sort, Warning};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
/// Resolved application directories for one profile.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Configuration directory.
    pub config: PathBuf,
    /// State directory.
    pub state: PathBuf,
}
/// Determine XDG paths. CLI profiles are validated before this call.
pub fn paths(profile: Option<&str>) -> Paths {
    let base = Paths {
        config: glib::user_config_dir().join("nib"),
        state: glib::user_state_dir().join("nib"),
    };
    match profile {
        Some(p) if !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
            Paths {
                config: base.config.join(p),
                state: base.state.join(p),
            }
        }
        _ => base,
    }
}
/// SHA-256 of the canonical root, first sixteen lowercase hex characters.
pub fn vault_key(root: &Path) -> Result<String, VaultError> {
    let root = root.canonicalize()?;
    let hash = glib::compute_checksum_for_string(
        glib::ChecksumType::Sha256,
        root.to_string_lossy().as_ref(),
    )
    .ok_or_else(|| VaultError::Message("SHA-256 unavailable".into()))?;
    Ok(hash[..16].into())
}
/// Resolve an explicit root, or the nearest vault marker above a note.
pub fn find_vault_root(explicit: Option<&Path>, opened: &Path) -> Result<PathBuf, VaultError> {
    if let Some(root) = explicit {
        return Ok(root.canonicalize()?);
    }
    if opened.is_dir() {
        return Ok(opened.canonicalize()?);
    }
    let parent = opened
        .parent()
        .ok_or_else(|| VaultError::Denied(opened.into()))?
        .canonicalize()?;
    for ancestor in parent.ancestors() {
        if ancestor.join(".obsidian").is_dir() || ancestor.join(".nib").is_dir() {
            return Ok(ancestor.into());
        }
    }
    Ok(parent)
}
/// Read config, atomically creating commented defaults on first run.
pub fn load_config(paths: &Paths, guard: &Guard) -> Result<(Config, Vec<Warning>), VaultError> {
    let file = paths.config.join("nib.toml");
    match std::fs::read_to_string(&file) {
        Ok(t) => Ok(Config::parse(&t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fsx::create_dir_all(guard, &paths.config)?;
            fsx::create(guard, &file, DEFAULT_TOML.as_bytes())?;
            Ok((Config::default(), vec![]))
        }
        Err(e) => Err(e.into()),
    }
}
/// Saved cursor and note per tab.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabState {
    /// Relative note ID, None for untitled.
    pub note: Option<nib_core::NoteId>,
    /// One-based line.
    pub line: u32,
    /// Byte column.
    pub column: u32,
    /// Reading mode.
    pub reading: bool,
}
/// Per-vault window state, never stored in the vault itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Open tabs.
    pub tabs: Vec<TabState>,
    /// Active tab index.
    pub active: usize,
    /// Pane positions in pixels.
    pub panes: Vec<i32>,
    /// Files visibility.
    pub files_visible: bool,
    /// Right-panel visibility.
    pub right_visible: bool,
    /// Optional parent column.
    pub parent_visible: bool,
    /// Relative browsed directory.
    pub folder: String,
    /// Cursor name in the directory.
    pub cursor: String,
    /// File ordering.
    pub sort: Sort,
    /// Reverse ordering.
    pub reversed: bool,
    /// Recent note IDs.
    pub recent: Vec<nib_core::NoteId>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            tabs: vec![],
            active: 0,
            panes: vec![],
            files_visible: true,
            right_visible: true,
            parent_visible: false,
            folder: String::new(),
            cursor: String::new(),
            sort: Sort::Name,
            reversed: false,
            recent: vec![],
        }
    }
}
impl Session {
    /// Corrupt or absent state falls back to defaults with a warning for corruption.
    pub fn load(file: &Path) -> (Self, Vec<Warning>) {
        match std::fs::read_to_string(file) {
            Ok(t) => match toml::from_str(&t) {
                Ok(s) => (s, vec![]),
                Err(e) => (Self::default(), vec![Warning(e.to_string())]),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), vec![]),
            Err(e) => (Self::default(), vec![Warning(e.to_string())]),
        }
    }
    /// Atomically persist state beneath the profile directory.
    pub fn save(&self, file: &Path, guard: &Guard) -> Result<(), VaultError> {
        if let Some(p) = file.parent() {
            fsx::create_dir_all(guard, p)?;
        }
        let t = toml::to_string(self).map_err(|e| VaultError::Message(e.to_string()))?;
        fsx::save(guard, file, t.as_bytes())?;
        Ok(())
    }
}
/// Append a sanitized crash report supplied by the binary, without note contents.
pub fn append_crash(paths: &Paths, guard: &Guard, report: &str) -> Result<(), VaultError> {
    fsx::create_dir_all(guard, &paths.state)?;
    fsx::append(guard, &paths.state.join("crash.log"), report.as_bytes())
}
pub use crate::fsx::cleanup_stale_tmp;
