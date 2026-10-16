//! Per-vault session (P§15): open tabs and their cursor lines, the active
//! tab, pane sizes, panel visibility, the files column's folder, cursor and
//! sort. Saved on close and every 30 s when it changed; restored on open
//! (the active tab before the first frame, the others just after it).
use crate::{app::App, gtk, reading};
use gtk::prelude::*;
use nib_vault::state::{Session, TabState};
use std::{rc::Rc, time::Duration};

const EVERY: Duration = Duration::from_secs(30);

fn file(app: &App) -> std::path::PathBuf {
    app.paths.state.join(&app.key).join("session.toml")
}

/// The session as the window shows it now.
pub fn snapshot(app: &App) -> Session {
    let mut session = app.session.borrow().clone();
    session.tabs = app
        .tabs
        .all()
        .iter()
        .filter(|t| !t.config.get())
        .map(|t| {
            let cursor = t.buffer.iter_at_mark(&t.buffer.get_insert());
            TabState {
                note: t
                    .path
                    .borrow()
                    .as_deref()
                    .and_then(|p| p.strip_prefix(app.vault.root()).ok())
                    .and_then(|p| p.to_str())
                    .and_then(|p| nib_core::NoteId::new(p).ok()),
                line: u32::try_from(cursor.line() + 1).unwrap_or(1),
                column: u32::try_from(cursor.line_offset()).unwrap_or(0),
                reading: t.mode.get() == reading::Mode::Reading,
            }
        })
        .collect();
    let active = app.tabs.current().id;
    session.active = app
        .tabs
        .all()
        .iter()
        .filter(|t| !t.config.get())
        .position(|t| t.id == active)
        .unwrap_or(0);
    session.panes = vec![app.outer.position(), app.inner.position()];
    session.folder = app
        .files
        .folder()
        .strip_prefix(app.vault.root())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    session.cursor = app.files.cursor_name().unwrap_or_default();
    session.files_visible = app.files.root.is_visible();
    session.right_visible = app.panel.root.is_visible();
    session.parent_visible = app.files.parent_visible();
    session
}

/// Write the session if it differs from what was last written.
pub fn save(app: &App) {
    let now = snapshot(app);
    if app.saved_session.borrow().as_ref() == Some(&now) {
        return;
    }
    match now.save(&file(app), &app.guard) {
        Ok(()) => *app.saved_session.borrow_mut() = Some(now),
        Err(e) => log::warn!("session: {e}"),
    }
}

pub fn start_timer(app: &Rc<App>) {
    let weak = app.this.clone();
    glib::timeout_add_local(EVERY, move || match weak.upgrade() {
        Some(app) => {
            save(&app);
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
}

/// After the first frame: the session's other tabs, in their order. When
/// the first tab shows a note named on the command line instead of the
/// session's active one, that tab stays current and comes last.
pub fn restore(app: &Rc<App>) {
    let session = app.session.borrow().clone();
    let first = app.tabs.current();
    let first_path = first.path.borrow().clone();
    let path_of = |state: &TabState| {
        state
            .note
            .as_ref()
            .map(|id| app.vault.root().join(id.filename()))
    };
    let active = session.active.min(session.tabs.len().saturating_sub(1));
    let explicit = session.tabs.get(active).map(path_of) != Some(first_path.clone());
    let mut ordered = Vec::with_capacity(session.tabs.len() + 1);
    for (i, state) in session.tabs.iter().enumerate() {
        let tab = if i == active && !explicit {
            first.clone()
        } else {
            let path = path_of(state);
            if path.is_some() && path == first_path {
                continue;
            }
            let Some(tab) = app.tabs.add_tab() else { break };
            if let Some(path) = path.filter(|p| p.is_file()) {
                app.load_into(&tab, &path, Some(state.line), false);
            }
            tab
        };
        if state.reading {
            reading::set_mode(app, &tab, reading::Mode::Reading);
        }
        ordered.push(tab.id);
    }
    if explicit {
        ordered.push(first.id);
    }
    app.tabs.reorder(&ordered);
    let current = ordered
        .iter()
        .position(|id| {
            *id == if explicit {
                first.id
            } else {
                ordered[active.min(ordered.len() - 1)]
            }
        })
        .unwrap_or(0);
    app.tabs.select(current);
    app.tabs.focus_current();
}
