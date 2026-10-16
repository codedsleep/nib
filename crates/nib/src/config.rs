//! `:config` (P§19): `nib.toml` in a tab. Saving it reloads the config and
//! applies fonts, scheme, wrap, autosave, margins and vim at once; vault
//! folders apply at the next vault open. Warnings go to the status line.
use crate::{app::App, editor, theme};
use nib_vault::{NoteText, state};
use sourceview5::prelude::*;
use std::rc::Rc;

pub fn open(app: &Rc<App>) {
    if let Some(i) = app.tabs.all().iter().position(|t| t.config.get()) {
        app.tabs.select(i);
        app.tabs.focus_current();
        return;
    }
    let text = match state::config_text(&app.paths, &app.guard) {
        Ok(t) => t,
        Err(e) => {
            app.status.error(&format!("cannot read nib.toml: {e}"));
            return;
        }
    };
    let tab = app.tabs.new_tab();
    tab.config.set(true);
    let path = app.paths.config.join("nib.toml");
    let note = NoteText {
        text,
        fmt: nib_core::text::TextFormat::default(),
        read_only: false,
        notice: None,
    };
    tab.load(app, &path, note, 1);
    if let Some(lang) = sourceview5::LanguageManager::default().language("toml") {
        tab.buffer.set_language(Some(&lang));
    }
    tab.refresh_label(app);
    app.update_title();
    tab.view.grab_focus();
}

/// The config tab's save: write, parse, apply.
pub fn save(app: &Rc<App>, tab: &editor::Tab) -> bool {
    let text = tab.text();
    if let Err(e) = state::save_config_text(&app.paths, &app.guard, &text) {
        app.status.error(&format!("cannot save nib.toml: {e}"));
        return false;
    }
    tab.buffer.set_modified(false);
    let (config, warnings) = nib_core::settings::Config::parse(&text);
    *app.config.borrow_mut() = config;
    apply(app);
    match warnings.first() {
        Some(w) => app.status.error(&format!(
            "nib.toml: {}{}",
            w.0,
            if warnings.len() > 1 {
                format!(" (+{} more)", warnings.len() - 1)
            } else {
                String::new()
            }
        )),
        None => app.status.notice("nib.toml reloaded"),
    }
    true
}

/// Apply what can change live.
pub fn apply(app: &Rc<App>) {
    let config = app.config.borrow().clone();
    theme::apply_fonts(&config);
    theme::restyle_reading(&config);
    for tab in app.tabs.all() {
        tab.reapply(app);
        crate::editor::vim::sync(app, &tab);
        if tab.reading.borrow().is_some() && tab.mode.get() != crate::reading::Mode::Source {
            crate::reading::rerender(app, &tab);
        }
    }
    app.files.rebuild();
}
