//! The desktop's GTK theme (D15), nib's stylesheet, the editor fonts and
//! the style-scheme search path.
//!
//! A theme with a GTK 4 stylesheet loads by itself. A GTK 3-only theme, such
//! as the owner's `NordArc`, would leave GTK 4 grey: its palette recolours
//! GTK's built-in stylesheet (`VaultHandle::theme_css`), which then replaces
//! the theme at the same priority.
use crate::gtk;
use gtk::prelude::*;
use std::{cell::Cell, path::PathBuf, rc::Rc};

/// A GTK theme with no rules at all.
const NO_THEME: &str = "Empty";

thread_local! {
    static EDITOR_CSS: gtk::CssProvider = gtk::CssProvider::new();
}

/// Once per process, before the first window.
pub fn install() {
    // Binds GtkSourceView's translations (vim's bar text) and its resources.
    sourceview5::init();
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    follow_desktop(&display);
    let own = gtk::CssProvider::new();
    own.load_from_string(include_str!("nib.css"));
    gtk::style_context_add_provider_for_display(
        &display,
        &own,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    EDITOR_CSS.with(|p| {
        gtk::style_context_add_provider_for_display(
            &display,
            p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    });
    let manager = sourceview5::StyleSchemeManager::default();
    for dir in scheme_dirs() {
        manager.append_search_path(&dir.to_string_lossy());
    }
}

/// Dark is the standard look; a theme without a dark variant is used as it is.
fn follow_desktop(display: &gtk::gdk::Display) {
    let settings = gtk::Settings::for_display(display);
    settings.set_gtk_application_prefer_dark_theme(true);
    if std::env::var_os("GTK_THEME").is_some() {
        return;
    }
    let named = std::env::var("NIB_GTK_THEME")
        .ok()
        .filter(|n| !n.is_empty());
    let Some(name) = named
        .clone()
        .or_else(|| settings.gtk_theme_name().map(Into::into))
    else {
        return;
    };
    let Some(theme) = nib_vault::theme::discover(&name, &nib_vault::theme::roots()) else {
        return;
    };
    if theme.native_gtk4 {
        settings.set_gtk_application_prefer_dark_theme(theme.dark_variant);
        if named.is_some() {
            settings.set_gtk_theme_name(Some(&name));
        }
        return;
    }
    let Some(css) = nib_vault::VaultHandle::theme_css(&name) else {
        return;
    };
    let provider = gtk::CssProvider::new();
    let errors = Rc::new(Cell::new(0_u32));
    let handler = provider.connect_parsing_error({
        let errors = errors.clone();
        move |_, section, error| {
            // Deprecation warnings arrive here too; the built-in stylesheet has some.
            if !error.is::<gtk::CssParserWarning>() {
                log::debug!("recoloured theme, {section}: {error}");
                errors.set(errors.get() + 1);
            }
        }
    });
    provider.load_from_string(&css);
    provider.disconnect(handler);
    if errors.get() > 0 {
        log::warn!(
            "theme {name}: the recoloured stylesheet has {} errors and is not used",
            errors.get()
        );
        return;
    }
    log::info!("theme {name} has no GTK 4 part; using its GTK 3 colours");
    settings.set_gtk_application_prefer_dark_theme(false);
    settings.set_gtk_theme_name(Some(NO_THEME));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_THEME,
    );
}

/// Style schemes beside a source build or an installed binary.
fn scheme_dirs() -> Vec<PathBuf> {
    let Ok(exe) = std::env::current_exe() else {
        return vec![];
    };
    let mut dirs = vec![];
    for up in exe.ancestors().skip(1).take(4) {
        for rel in ["data/style-schemes", "share/nib/style-schemes"] {
            let dir = up.join(rel);
            if dir.join("nib-nord.xml").is_file() {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// A `GSettings` string, or None when the schema is not installed (looking it
/// up first: `gio::Settings::new` aborts on an unknown schema).
fn desktop_setting(key: &str) -> Option<String> {
    let schema = "org.gnome.desktop.interface";
    let source = gio::SettingsSchemaSource::default()?;
    source.lookup(schema, true)?.has_key(key).then_some(())?;
    let value: String = gio::Settings::new(schema).string(key).into();
    (!value.is_empty()).then_some(value)
}

/// CSS `font` declarations for a Pango font name, with a point-size delta.
fn font_css(name: &str, delta: f64) -> String {
    let desc = gtk::pango::FontDescription::from_string(name);
    let family = desc
        .family()
        .map_or_else(|| "monospace".into(), |f| f.to_string());
    let size = if desc.size() > 0 {
        f64::from(desc.size()) / f64::from(gtk::pango::SCALE)
    } else {
        11.0
    };
    format!(
        "font-family: \"{}\"; font-size: {}pt;",
        family.replace('"', ""),
        size + delta
    )
}

/// Apply `editor.font` / `editor.reading_font` (or the desktop's fonts).
pub fn apply_fonts(config: &nib_core::settings::Config) {
    let mono = if config.editor.font.is_empty() {
        desktop_setting("monospace-font-name").unwrap_or_else(|| "Monospace 11".into())
    } else {
        config.editor.font.clone()
    };
    let reading = if config.editor.reading_font.is_empty() {
        let ui = desktop_setting("font-name").unwrap_or_else(|| "Sans 11".into());
        font_css(&ui, 1.0)
    } else {
        font_css(&config.editor.reading_font, 0.0)
    };
    let css = format!(
        "textview.nib-editor {{ {} }}\ntextview.nib-reading {{ {reading} }}\n",
        font_css(&mono, 0.0)
    );
    EDITOR_CSS.with(|p| p.load_from_string(&css));
}

/// The configured scheme, falling back to nib's own and then GTK's.
pub fn scheme(id: &str) -> Option<sourceview5::StyleScheme> {
    let manager = sourceview5::StyleSchemeManager::default();
    [id, "nib-nord", "Adwaita-dark"]
        .iter()
        .find_map(|id| manager.scheme(id))
}

#[cfg(test)]
mod tests {
    #[test]
    fn font_css() {
        assert_eq!(
            super::font_css("JetBrains Mono 12", 0.0),
            "font-family: \"JetBrains Mono\"; font-size: 12pt;"
        );
        assert_eq!(
            super::font_css("Cantarell 11", 1.0),
            "font-family: \"Cantarell\"; font-size: 12pt;"
        );
    }
}
