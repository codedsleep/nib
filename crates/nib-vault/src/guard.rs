//! Canonical path confinement, including uncreated tails and symlink ancestors.
use crate::VaultError;
use std::path::{Component, Path, PathBuf};
/// Resolve existing ancestors and reject lexical escapes before checking prefixes.
pub fn resolve(path: &Path) -> Result<PathBuf, VaultError> {
    if !path.is_absolute() {
        return Err(VaultError::Denied(path.into()));
    }
    // Resolve each component before processing `..`, preserving kernel symlink semantics.
    let mut out = PathBuf::from("/");
    for c in path.components() {
        match c {
            Component::Normal(s) => {
                out.push(s);
                if std::fs::symlink_metadata(&out).is_ok() {
                    out = std::fs::canonicalize(&out)?;
                }
            }
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir | Component::RootDir => {}
            Component::Prefix(_) => return Err(VaultError::Denied(path.into())),
        }
    }
    Ok(out)
}
/// Write policy. Unset environment means an ordinary unconfined application.
#[derive(Debug, Clone)]
pub struct Guard {
    roots: Option<Vec<PathBuf>>,
}
impl Guard {
    /// Read `NIB_ALLOW_ROOTS`, expanding a leading tilde in each colon-delimited root.
    pub fn from_env() -> Self {
        let roots = std::env::var_os("NIB_ALLOW_ROOTS").map(|s| {
            let home = glib::home_dir();
            let mut roots = vec![
                glib::user_cache_dir().join("nib-fixture"),
                std::env::temp_dir(),
            ];
            for s in s.to_string_lossy().split(':').filter(|s| !s.is_empty()) {
                roots.push(if s == "~" {
                    home.clone()
                } else if let Some(p) = s.strip_prefix("~/") {
                    home.join(p)
                } else {
                    PathBuf::from(s)
                });
            }
            roots
        });
        Self { roots }
    }
    /// Explicit confined policy for tests and embedded use; no implicit extra roots.
    pub fn confined(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots) }
    }
    /// Explicit ordinary-user policy.
    pub fn unconfined() -> Self {
        Self { roots: None }
    }
    /// Verify the resolved target is inside an allowed root.
    pub fn check(&self, path: &Path) -> Result<(), VaultError> {
        let resolved = resolve(path)?;
        if self.roots.as_ref().is_none_or(|rs| {
            rs.iter()
                .any(|r| resolve(r).is_ok_and(|r| resolved.starts_with(r)))
        }) {
            Ok(())
        } else {
            Err(VaultError::Denied(path.into()))
        }
    }
}
