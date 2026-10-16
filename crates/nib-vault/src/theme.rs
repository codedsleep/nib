//! Local GTK theme discovery and bounded, cycle-safe CSS imports.
use nib_core::palette::{self, Palette};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
/// Theme facts needed by the frontend before loading a provider.
pub struct Theme {
    /// Parsed named colors from the preferred GTK 3 variant.
    pub palette: Palette,
    /// Theme includes native GTK 4 CSS.
    pub native_gtk4: bool,
    /// Native GTK 4 dark variant exists.
    pub dark_variant: bool,
}
/// Search desktop and pre-isolation XDG locations without modifying them.
pub fn roots() -> Vec<PathBuf> {
    let mut roots = vec![
        glib::home_dir().join(".themes"),
        glib::user_data_dir().join("themes"),
        glib::home_dir().join(".local/share/themes"),
    ];
    if let Some(host) = std::env::var_os("NIB_HOST_XDG_DATA_HOME") {
        roots.push(PathBuf::from(host).join("themes"));
    }
    roots.extend(
        glib::system_data_dirs()
            .into_iter()
            .map(|p| p.join("themes")),
    );
    roots
}
fn read_imports(
    file: &Path,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
    out: &mut String,
) {
    if depth > 8 {
        return;
    }
    let Ok(path) = file.canonicalize() else {
        return;
    };
    if !path.starts_with(root) || !seen.insert(path.clone()) {
        return;
    }
    let Ok(own) = std::fs::read_to_string(&path) else {
        return;
    };
    for import in own.split("@import").skip(1) {
        let value = import.split(';').next().unwrap_or_default().trim();
        let value = value
            .strip_prefix("url(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(value)
            .trim()
            .trim_matches(['\'', '"']);
        if !value.contains(':')
            && !Path::new(value).is_absolute()
            && let Some(parent) = path.parent()
        {
            read_imports(&parent.join(value), root, seen, depth + 1, out);
        }
    }
    out.push_str(&own);
    out.push('\n');
}
/// Discover one named theme in explicit roots (no environment mutation in tests).
pub fn discover(name: &str, roots: &[PathBuf]) -> Option<Theme> {
    if name.is_empty() || Path::new(name).file_name() != Some(std::ffi::OsStr::new(name)) {
        return None;
    }
    let root = roots
        .iter()
        .map(|r| r.join(name))
        .find(|p| p.is_dir())?
        .canonicalize()
        .ok()?;
    let mut text = String::new();
    for file in ["gtk-dark.css", "gtk.css"] {
        read_imports(
            &root.join("gtk-3.0").join(file),
            &root,
            &mut HashSet::new(),
            0,
            &mut text,
        );
        if !text.is_empty() {
            break;
        }
    }
    let native: Vec<_> = std::fs::read_dir(&root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("gtk-4."))
        .map(|e| e.path())
        .collect();
    Some(Theme {
        palette: palette::parse_gtk3_colors(&text),
        native_gtk4: native.iter().any(|p| p.join("gtk.css").exists()),
        dark_variant: native.iter().any(|p| p.join("gtk-dark.css").exists()),
    })
}
/// Recolor a caller-supplied GTK base; this also supports display-free tests.
pub fn theme_css_with_base(name: &str, base: &str) -> Option<String> {
    let name = std::env::var("NIB_GTK_THEME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.into());
    let theme = discover(&name, &roots())?;
    if theme.palette.is_empty() || theme.native_gtk4 {
        return None;
    }
    Some(palette::recolour(base, &theme.palette).replace(
        "url(\"assets/",
        "url(\"resource:///org/gtk/libgtk/theme/Default/assets/",
    ))
}
/// Recolor GTK's registered base resource. Call after linking/initializing GTK;
/// display-free callers use `theme_css_with_base` with an extracted base.
pub fn theme_css(name: &str) -> Option<String> {
    let selected = std::env::var("NIB_GTK_THEME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.into());
    let theme = discover(&selected, &roots())?;
    if theme.native_gtk4 || theme.palette.is_empty() {
        return None;
    }
    let variant = if theme.palette.is_dark().unwrap_or(true) {
        "dark"
    } else {
        "light"
    };
    let bytes = gio::resources_lookup_data(
        &format!("/org/gtk/libgtk/theme/Default/Default-{variant}.css"),
        gio::ResourceLookupFlags::NONE,
    )
    .ok()?;
    theme_css_with_base(&selected, std::str::from_utf8(&bytes).ok()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports() {
        let d = tempfile::tempdir().unwrap();
        let theme = d.path().join("Test/gtk-3.0");
        std::fs::create_dir_all(&theme).unwrap();
        std::fs::write(
            theme.join("gtk.css"),
            "@import url(\"colors.css\");\n@define-color theme_bg_color #123456;",
        )
        .unwrap();
        std::fs::write(theme.join("colors.css"),"@import 'gtk.css';\n@define-color theme_fg_color #ffffff;\n@define-color theme_selected_bg_color #abcdef;").unwrap();
        let t = discover("Test", &[d.path().into()]).unwrap();
        assert_eq!(t.palette.is_dark(), Some(true));
        assert_eq!(
            t.palette.color("theme_fg_color"),
            Some("rgb(255, 255, 255)".into())
        );
        assert!(!t.native_gtk4);
        assert!(discover("../Test", &[d.path().into()]).is_none());
    }
}
