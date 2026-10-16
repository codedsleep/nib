//! Following links (P§9, D10): notes open in nib, unresolved links create
//! their note, web and mail links go to the desktop, and attachments open
//! only when the launch guard allows it. Every follow surface comes here.
use crate::{app::App, gtk};
use gtk::prelude::*;
use nib_core::{
    LinkTarget, NoteId,
    launch::{self, Launch},
    links,
};
use std::{os::unix::fs::PermissionsExt, path::Path, rc::Rc};

/// `Alt+Enter`, `:follow`, `Ctrl+click` land here through the cursor.
pub fn follow_cursor(app: &Rc<App>) {
    let tab = app.tabs.current();
    let iter = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
    follow_at(app, &iter);
}

/// Follow the link whose source span holds `iter`.
pub fn follow_at(app: &Rc<App>, iter: &gtk::TextIter) {
    let tab = app.tabs.current();
    let text = tab.text();
    let offset = tab.buffer.text(&tab.buffer.start_iter(), iter, true).len();
    let Some((span, raw)) = nib_core::edit::link_at(&text, offset) else {
        app.status.notice("no link under the cursor");
        return;
    };
    let source = &text[span];
    let from = tab.path.borrow().as_deref().and_then(|p| app.note_id(p));
    let target = if source.starts_with("[[") || source.starts_with("![[") {
        if !index_ready(app) {
            return;
        }
        let from = from.unwrap_or_else(root_id);
        nib_core::resolve(&app.index.borrow(), &from, &raw)
    } else if source.starts_with('[') || source.starts_with("![") {
        if !index_ready(app) {
            return;
        }
        let from = from.unwrap_or_else(root_id);
        links::resolve_markdown(&app.index.borrow(), &from, &raw)
    } else {
        LinkTarget::External(raw.clone())
    };
    follow(app, &target, &raw);
}

/// The root folder stands in as the source of an untitled tab's links.
fn root_id() -> NoteId {
    NoteId::new("untitled").expect("a constant valid id")
}

fn index_ready(app: &App) -> bool {
    if !app.index_ready.get() {
        app.status
            .notice("index not ready — links resolve once indexing finishes");
    }
    app.index_ready.get()
}

/// Open a resolved target.
pub fn follow(app: &Rc<App>, target: &LinkTarget, raw: &str) {
    match target {
        LinkTarget::Note { id, heading, block } => {
            let path = app.vault.root().join(id.filename());
            app.open_note(&path, false, None);
            if let Some(h) = heading {
                scroll_to_heading(app, id, h);
            } else if let Some(b) = block {
                scroll_to_block(app, id, b);
            }
        }
        LinkTarget::Unresolved(name) => create_unresolved(app, name),
        LinkTarget::External(url) => open_url(app, url),
        LinkTarget::Attachment(rel) => open_attachment(app, &app.vault.root().join(rel)),
    }
    let _ = raw;
}

fn scroll_to_heading(app: &App, id: &NoteId, slug: &str) {
    let line = app
        .index
        .borrow()
        .notes
        .get(id)
        .and_then(|m| m.headings.iter().find(|h| h.slug == slug).map(|h| h.line));
    match line {
        Some(line) => app.tabs.current().goto_line(line),
        None => app.status.notice(&format!("no heading #{slug} in {id}")),
    }
}

fn scroll_to_block(app: &App, id: &NoteId, block: &str) {
    let line = app
        .index
        .borrow()
        .notes
        .get(id)
        .and_then(|m| m.blocks.iter().find(|(b, _)| b == block).map(|(_, l)| *l));
    match line {
        Some(line) => app.tabs.current().goto_line(line),
        None => app.status.notice(&format!("no block ^{block} in {id}")),
    }
}

/// The folder new notes go to (`vault.new_folder`: "" root, "current", or a folder).
pub fn new_note_folder(app: &App) -> String {
    let folder = app.config.borrow().vault.new_folder.clone();
    if folder == "current" {
        let tab = app.tabs.current();
        let current = tab
            .path
            .borrow()
            .as_deref()
            .and_then(|p| app.note_id(p))
            .map(|id| id.folder().to_owned());
        return current.unwrap_or_default();
    }
    folder.trim_matches('/').to_owned()
}

/// A note id for a typed or linked name in the new-note folder (a name with
/// a slash is taken as a vault-relative path).
pub fn new_note_id(app: &App, name: &str) -> Option<NoteId> {
    let name = name.split(['#', '|']).next().unwrap_or_default().trim();
    if name.is_empty() {
        return None;
    }
    let rel = if name.contains('/') {
        name.to_owned()
    } else {
        let folder = new_note_folder(app);
        if folder.is_empty() {
            name.to_owned()
        } else {
            format!("{folder}/{name}")
        }
    };
    NoteId::new(&rel).ok()
}

/// Obsidian's behaviour: following an unresolved link creates the note.
pub fn create_unresolved(app: &Rc<App>, name: &str) {
    let Some(id) = new_note_id(app, name) else {
        app.status
            .error(&format!("cannot make a note called {name}"));
        return;
    };
    match app.vault.create_note(&id, "") {
        Ok(path) => {
            app.status.notice(&format!("created {}", id.filename()));
            app.open_note(&path, false, None);
        }
        Err(e) => app
            .status
            .error(&format!("cannot create {}: {e}", id.filename())),
    }
}

/// `:e NAME`: an existing note by wikilink rules or path, else a new one.
pub fn open_name(app: &Rc<App>, name: &str) {
    let name = name.trim();
    let candidate = app.vault.root().join(name);
    let md = Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if (name.contains('/') || md) && !name.starts_with('/') {
        // A path: it must stay inside the vault.
        match app.vault.checked_path(&candidate) {
            Ok(p) if p.is_file() => {
                app.open_note(&p, false, None);
                return;
            }
            Ok(_) => {}
            Err(_) => {
                app.status.error(&format!("{name} is outside the vault"));
                return;
            }
        }
    } else if name.starts_with('/') {
        app.status.error(&format!("{name} is outside the vault"));
        return;
    }
    if !index_ready(app) {
        return;
    }
    let from = app
        .tabs
        .current()
        .path
        .borrow()
        .as_deref()
        .and_then(|p| app.note_id(p))
        .unwrap_or_else(root_id);
    let target = nib_core::resolve(&app.index.borrow(), &from, name.trim_end_matches(".md"));
    follow(app, &target, name);
}

/// Web and mail links only (D10).
pub fn open_url(app: &Rc<App>, url: &str) {
    if !launch::url_allowed(url) {
        app.status.error(&format!(
            "refused to open {url} — only http, https and mailto links open"
        ));
        return;
    }
    let context = host_context(app);
    match gio::AppInfo::launch_default_for_uri(url, Some(&context)) {
        Ok(()) => app.status.notice(&format!("opened {url}")),
        Err(e) => app.status.error(&format!("cannot open {url}: {e}")),
    }
}

/// Attachments: images in nib (Phase 8), everything else through the guard.
pub fn open_attachment(app: &Rc<App>, path: &Path) {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let path = match app.vault.checked_path(path) {
        Ok(p) => p,
        Err(e) => {
            app.status.error(&format!("cannot open {name}: {e}"));
            return;
        }
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        app.status.error(&format!("{name} does not exist"));
        return;
    };
    let file = gio::File::for_path(&path);
    let content_type = file
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|i| i.content_type())
        .map(|t| t.to_string())
        .unwrap_or_default();
    if let Launch::Refuse(reason) =
        launch::classify(&name, meta.permissions().mode(), &content_type)
    {
        app.status.error(&reason);
        return;
    }
    let context = host_context(app);
    match gio::AppInfo::launch_default_for_uri(&file.uri(), Some(&context)) {
        Ok(()) => app.status.notice(&format!("opened {name}")),
        Err(e) => app.status.error(&format!("cannot open {name}: {e}")),
    }
}

/// A launch context with the environment nib was started from (S7):
/// `dev-run.sh` exports `NIB_HOST_X`, or `NIB_HOST_X__UNSET=1` for an unset X.
fn host_context(app: &App) -> gio::AppLaunchContext {
    let context = WidgetExt::display(&app.window).app_launch_context();
    for (key, value) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let Some(name) = key.strip_prefix("NIB_HOST_") else {
            continue;
        };
        if let Some(name) = name.strip_suffix("__UNSET") {
            context.unsetenv(name);
        } else {
            context.setenv(name, value);
        }
        context.unsetenv(key);
    }
    context.upcast()
}
