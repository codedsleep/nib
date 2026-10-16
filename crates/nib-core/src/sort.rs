//! Natural directory ordering and invariant vault visibility rules.
use crate::settings::Sort;
use std::{
    cmp::Ordering,
    path::PathBuf,
    time::{Duration, SystemTime},
};
/// A file-list row without toolkit objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Absolute path supplied by vault.
    pub path: PathBuf,
    /// Display filename.
    pub name: String,
    /// Folder or folder symlink.
    pub directory: bool,
    /// Modification time.
    pub modified: SystemTime,
    /// Byte size.
    pub size: u64,
}
/// Internal vault directories never appear, even with hidden files on.
pub fn never_shown(name: &str) -> bool {
    [".obsidian", ".nib", ".git", ".trash"].contains(&name)
        || (name.starts_with('.') && name.ends_with(".nib-tmp"))
}
/// Visibility is a pure preference applied to cached listings.
pub fn visible(entry: &Entry, hidden: bool, only_markdown: bool) -> bool {
    !never_shown(&entry.name)
        && (hidden || !entry.name.starts_with('.'))
        && (!only_markdown || entry.directory || entry.name.to_ascii_lowercase().ends_with(".md"))
}
/// Compare digit runs numerically without integer overflow.
pub fn natural(a: &str, b: &str) -> Ordering {
    let aa = a.to_lowercase();
    let bb = b.to_lowercase();
    let (mut a, mut b) = (aa.as_bytes(), bb.as_bytes());
    while !a.is_empty() && !b.is_empty() {
        if a[0].is_ascii_digit() && b[0].is_ascii_digit() {
            let na = a.iter().take_while(|c| c.is_ascii_digit()).count();
            let nb = b.iter().take_while(|c| c.is_ascii_digit()).count();
            let x = &a[..na];
            let y = &b[..nb];
            let x = &x[x.iter().take_while(|c| **c == b'0').count()..];
            let y = &y[y.iter().take_while(|c| **c == b'0').count()..];
            let ord = x
                .len()
                .cmp(&y.len())
                .then_with(|| x.cmp(y))
                .then(na.cmp(&nb));
            if ord != Ordering::Equal {
                return ord;
            }
            a = &a[na..];
            b = &b[nb..];
        } else {
            let ord = a[0].cmp(&b[0]);
            if ord != Ordering::Equal {
                return ord;
            }
            a = &a[1..];
            b = &b[1..];
        }
    }
    a.len().cmp(&b.len())
}
/// Sort folders first and reverse within each group when requested.
pub fn sort_entries(entries: &mut [Entry], order: Sort, reversed: bool) {
    entries.sort_by(|a, b| {
        b.directory.cmp(&a.directory).then_with(|| {
            let o = if order == Sort::Modified {
                b.modified
                    .cmp(&a.modified)
                    .then_with(|| natural(&a.name, &b.name))
            } else {
                natural(&a.name, &b.name)
            };
            if reversed { o.reverse() } else { o }
        })
    });
}
/// Compact file age for the modified linemode.
pub fn age_label(modified: SystemTime, now: SystemTime) -> String {
    let s = now
        .duration_since(modified)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    if s < 60 {
        "now".into()
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordering() {
        assert_eq!(natural("note2", "note10"), Ordering::Less);
        assert_eq!(natural("note0002", "note10"), Ordering::Less);
        assert_eq!(
            age_label(
                SystemTime::UNIX_EPOCH,
                SystemTime::UNIX_EPOCH + Duration::from_secs(10800)
            ),
            "3h"
        );
        for n in [".obsidian", ".git", ".nib", ".trash"] {
            let e = Entry {
                path: n.into(),
                name: n.into(),
                directory: true,
                modified: SystemTime::UNIX_EPOCH,
                size: 0,
            };
            assert!(!visible(&e, true, false));
        }
    }
}
