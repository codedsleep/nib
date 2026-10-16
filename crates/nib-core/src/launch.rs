//! Pure attachment launch policy and path containment on canonical inputs.
use std::path::Path;
/// Whether the app may delegate opening to an external handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Safe to ask the desktop to open it.
    Open,
    /// Refusal notice.
    Refuse(String),
}
/// Reject executable bits, executable suffixes and executable MIME types.
pub fn classify(name: &str, mode: u32, content_type: &str) -> Launch {
    let lower = name.to_lowercase();
    if mode & 0o111 != 0
        || [".desktop", ".appimage", ".run", ".sh"]
            .iter()
            .any(|s| lower.ends_with(s))
        || [
            "application/x-executable",
            "application/x-sharedlib",
            "application/x-shellscript",
            "application/x-desktop",
        ]
        .contains(&content_type)
    {
        Launch::Refuse(format!("refused to run {name} — cp copies its path"))
    } else {
        Launch::Open
    }
}
/// Only web and mail protocols are delegated to the desktop.
pub fn url_allowed(url: &str) -> bool {
    url.split_once(':').is_some_and(|(s, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|a| s.eq_ignore_ascii_case(a))
    }) && !url.chars().any(char::is_control)
}
/// Prefix check on already canonical absolute inputs; I/O canonicalization is in vault.
pub fn within_vault(root: &Path, path: &Path) -> bool {
    root.is_absolute()
        && path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        && path.starts_with(root)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy() {
        for n in ["evil.sh", "X.AppImage", "a.run", "a.desktop"] {
            assert!(matches!(classify(n, 0, "text/plain"), Launch::Refuse(_)));
        }
        for t in [
            "application/x-executable",
            "application/x-sharedlib",
            "application/x-shellscript",
            "application/x-desktop",
        ] {
            assert!(matches!(classify("a", 0, t), Launch::Refuse(_)));
        }
        assert!(matches!(
            classify("a", 0o755, "text/plain"),
            Launch::Refuse(_)
        ));
        assert_eq!(classify("a.png", 0o644, "image/png"), Launch::Open);
        for u in ["file:/tmp/a", "javascript:bad", "custom:a", "https:a\n"] {
            assert!(!url_allowed(u));
        }
        assert!(url_allowed("HTTPS://example.com"));
        assert!(!within_vault(
            Path::new("/vault"),
            Path::new("/vault/../etc")
        ));
    }
}
