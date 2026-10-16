//! Attachments (P§14, G2): a pasted screenshot becomes
//! `Pasted image YYYYMMDDHHMMSS.png` in the attachments folder with an
//! `![[…]]` at the cursor; dropped or chosen files are copied in (a file
//! already in the vault is linked, not copied).
use crate::{app::App, clipboard, editor::Tab, fileops, gtk};
use gtk::prelude::*;
use nib_vault::OpSummary;
use std::{path::PathBuf, rc::Rc};

/// `Ctrl+V` in insert mode (or with vim off) when the clipboard holds an
/// image, `:paste-image` and the palette in any mode.
pub fn paste_image(app: &Rc<App>) {
    let tab = app.tabs.current();
    if !tab.view.is_editable() {
        app.status.error("this tab is read-only");
        return;
    }
    let id = tab.id;
    let weak = app.this.clone();
    glib::spawn_future_local(async move {
        let texture = clipboard::read_texture().await;
        let Some(app) = weak.upgrade() else { return };
        let Some(texture) = texture else {
            app.status.notice("no image on the clipboard");
            return;
        };
        let bytes = texture.save_to_png_bytes();
        let name = nib_core::attach::pasted_image_name(chrono::Local::now());
        match app.vault.write_attachment(&name, &bytes) {
            Ok(path) => {
                if let Some(tab) = app.tabs.by_id(id) {
                    insert_links(&app, &tab, &[path]);
                }
            }
            Err(e) => app.status.error(&format!("cannot save the image: {e}")),
        }
    });
}

/// Whether a key press is the image paste: `Ctrl+V` alone with an image on
/// the clipboard. Otherwise the key goes on to vim or GTK.
pub fn wants_ctrl_v(keyval: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> bool {
    use gtk::gdk::ModifierType as M;
    let mods = state & (M::CONTROL_MASK | M::SHIFT_MASK | M::ALT_MASK);
    mods == M::CONTROL_MASK && keyval.to_lower() == gtk::gdk::Key::v && clipboard::has_texture()
}

/// Links for files now in the vault, at the cursor, one per file.
fn insert_links(app: &Rc<App>, tab: &Tab, paths: &[PathBuf]) {
    let fmt = app.config.borrow().links.format;
    let from = tab.path.borrow().as_deref().and_then(|p| app.note_id(p));
    let links: Vec<String> = paths
        .iter()
        .map(|p| {
            if let Some(id) = app.note_id(p) {
                let target = if app.index_ready.get() {
                    match &from {
                        Some(from) => {
                            nib_core::links::link_from(&app.index.borrow(), from, &id, fmt)
                        }
                        None => nib_core::shortest_link(&app.index.borrow(), &id, fmt),
                    }
                } else {
                    id.to_string()
                };
                format!("[[{target}]]")
            } else {
                let rel = app.relative(p);
                let name =
                    nib_core::links::attachment_link(&app.index.borrow(), from.as_ref(), &rel, fmt);
                let bang = if crate::reading::is_image(p) { "!" } else { "" };
                format!("{bang}[[{name}]]")
            }
        })
        .collect();
    let buffer = &tab.buffer;
    buffer.begin_user_action();
    buffer.insert_at_cursor(&links.join("\n"));
    buffer.end_user_action();
    app.status.notice(&format!("inserted {}", links.join(" ")));
}

/// Dropped or chosen files: link what is in the vault, copy the rest in.
pub fn add_files(app: &Rc<App>, files: &[PathBuf]) {
    let tab = app.tabs.current();
    if !tab.view.is_editable() {
        app.status.error("this tab is read-only");
        return;
    }
    let root = app.vault.root().to_owned();
    let (inside, outside): (Vec<PathBuf>, Vec<PathBuf>) = files
        .iter()
        .cloned()
        .partition(|p| p.canonicalize().is_ok_and(|c| c.starts_with(&root)));
    if !inside.is_empty() {
        let inside: Vec<PathBuf> = inside
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        insert_links(app, &tab, &inside);
    }
    for source in outside {
        let id = app.vault.copy_attachment(&source);
        app.ops
            .borrow_mut()
            .insert(id, fileops::Pending::Attachment { tab: tab.id });
    }
}

/// A `CopyAttachment` finished: link where it landed.
pub fn copied(app: &Rc<App>, tab: u64, summary: &OpSummary) {
    if let Some(tab) = app.tabs.by_id(tab)
        && !summary.created.is_empty()
    {
        insert_links(app, &tab, &summary.created);
    }
}

/// Accept files dropped on a view.
pub fn drop_target(app: &Rc<App>, view: &impl IsA<gtk::Widget>) {
    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    let weak = app.this.clone();
    target.connect_drop(move |_, value, _, _| {
        let (Some(app), Ok(list)) = (weak.upgrade(), value.get::<gtk::gdk::FileList>()) else {
            return false;
        };
        let paths: Vec<PathBuf> = list.files().iter().filter_map(gio::File::path).collect();
        add_files(&app, &paths);
        true
    });
    view.add_controller(target);
}

/// `:attach`: a file chooser for the same.
pub fn choose(app: &Rc<App>) {
    let dialog = gtk::FileDialog::builder()
        .title("Attach files")
        .modal(true)
        .build();
    let weak = app.this.clone();
    dialog.open_multiple(
        Some(&app.window),
        None::<&gio::Cancellable>,
        move |result| {
            let (Some(app), Ok(model)) = (weak.upgrade(), result) else {
                return;
            };
            let paths: Vec<PathBuf> = (0..model.n_items())
                .filter_map(|i| model.item(i).and_downcast::<gio::File>())
                .filter_map(|f| f.path())
                .collect();
            add_files(&app, &paths);
        },
    );
}
