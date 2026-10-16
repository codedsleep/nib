//! The files column (P§8): one virtualised `ListView` over the listing the
//! vault's listing thread delivers, driven with vim motions.
use crate::{app::App, gtk, registry};
use gtk::prelude::*;
use nib_core::sort::{self, Entry};
use nib_vault::state::Session;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
};

/// One row: the entry, its display name, its selection and cut marks, and
/// the name label it is bound to (so a mark repaints just that row; nib
/// forbids `unsafe`, which rules out a `GObject` item with properties).
pub struct Row {
    pub entry: Entry,
    pub label: String,
    pub marked: Cell<bool>,
    pub cut: Cell<bool>,
    bound: RefCell<Option<glib::WeakRef<gtk::Label>>>,
}

impl Row {
    fn new(entry: Entry) -> Self {
        let label = if entry.directory {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        Self {
            entry,
            label,
            marked: Cell::new(false),
            cut: Cell::new(false),
            bound: RefCell::new(None),
        }
    }

    fn paint(&self) {
        if let Some(name) = self
            .bound
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
        {
            paint(&name, self);
        }
    }
}

fn paint(name: &gtk::Label, row: &Row) {
    for (class, on) in [("marked", row.marked.get()), ("cut", row.cut.get())] {
        if name.has_css_class(class) != on {
            if on {
                name.add_css_class(class);
            } else {
                name.remove_css_class(class);
            }
        }
    }
}

pub struct Files {
    pub root: gtk::Box,
    breadcrumb: gtk::Label,
    pub store: gio::ListStore,
    pub selection: gtk::SingleSelection,
    pub list: gtk::ListView,
    scroller: gtk::ScrolledWindow,
    vault_root: PathBuf,
    folder: RefCell<PathBuf>,
    listing: RefCell<Arc<[Entry]>>,
    pending: RefCell<String>,
    want: RefCell<Option<String>>,
    /// Visual mode: its anchor row and the marks from before it started.
    visual: RefCell<Option<(u32, Vec<bool>)>>,
    pub linemode: Cell<bool>,
    app: Weak<App>,
}

pub fn row(obj: &glib::Object) -> Option<std::cell::Ref<'_, Row>> {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .and_then(|b| b.try_borrow::<Row>().ok())
}

impl Files {
    pub fn new(app: &Weak<App>, vault_root: &Path, session: &Session) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("nib-files");
        root.set_width_request(120);
        let breadcrumb = gtk::Label::new(Some("~"));
        breadcrumb.add_css_class("nib-breadcrumb");
        breadcrumb.set_xalign(0.0);
        breadcrumb.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let factory = gtk::SignalListItemFactory::new();
        let weak = app.clone();
        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let name = gtk::Label::new(None);
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            let age = gtk::Label::new(None);
            age.add_css_class("dim");
            line.append(&name);
            line.append(&age);
            item.set_child(Some(&line));
        });
        factory.connect_bind(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let (Some(obj), Some(line)) = (item.item(), item.child()) else {
                return;
            };
            let Some(row) = row(&obj) else { return };
            let Some(name) = line.first_child().and_downcast::<gtk::Label>() else {
                return;
            };
            name.set_text(&row.label);
            paint(&name, &row);
            *row.bound.borrow_mut() = Some(name.downgrade());
            if let Some(age) = line.last_child().and_downcast::<gtk::Label>() {
                let show = weak.upgrade().is_some_and(|a| a.files.linemode.get());
                if show {
                    age.set_text(&sort::age_label(
                        row.entry.modified,
                        std::time::SystemTime::now(),
                    ));
                } else {
                    age.set_text("");
                }
            }
        });
        factory.connect_unbind(|_, item| {
            if let Some(obj) = item
                .downcast_ref::<gtk::ListItem>()
                .and_then(gtk::ListItem::item)
                && let Some(row) = row(&obj)
            {
                row.bound.borrow_mut().take();
            }
        });
        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        list.set_single_click_activate(false);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        root.append(&breadcrumb);
        root.append(&scroller);
        let folder = vault_root.join(&session.folder);
        let folder = if folder.is_dir() && folder.starts_with(vault_root) {
            folder
        } else {
            vault_root.to_owned()
        };
        let files = Self {
            root,
            breadcrumb,
            store,
            selection,
            list,
            scroller,
            vault_root: vault_root.to_owned(),
            folder: RefCell::new(folder),
            listing: RefCell::new(Arc::from(Vec::new())),
            pending: RefCell::new(String::new()),
            visual: RefCell::new(None),
            want: RefCell::new((!session.cursor.is_empty()).then(|| session.cursor.clone())),
            linemode: Cell::new(false),
            app: app.clone(),
        };
        files.connect();
        files
    }

    fn connect(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.app.clone();
        keys.connect_key_pressed(move |_, keyval, _, state| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            app.files.key(&app, keyval, state)
        });
        self.root.add_controller(keys);
        let weak = self.app.clone();
        self.list.connect_activate(move |_, position| {
            if let Some(app) = weak.upgrade() {
                app.files.selection.set_selected(position);
                app.files.open(&app);
            }
        });
    }

    /// Post-present start: list the remembered folder (or the opened note's).
    pub fn start(&self, app: &Rc<App>) {
        let tab = app.tabs.current();
        let note = tab.path.borrow().clone();
        if let Some(note) = note
            && let Some(parent) = note.parent()
        {
            *self.want.borrow_mut() = note.file_name().map(|n| n.to_string_lossy().into_owned());
            parent.clone_into(&mut self.folder.borrow_mut());
        }
        let folder = self.folder.borrow().clone();
        self.show_keep(app, &folder);
    }

    pub fn folder(&self) -> PathBuf {
        self.folder.borrow().clone()
    }

    /// Show a folder with the cursor on its first row.
    pub fn show(&self, app: &Rc<App>, folder: &Path) {
        *self.want.borrow_mut() = None;
        self.show_keep(app, folder);
    }

    /// Show a folder, keeping any cursor name already asked for.
    pub fn show_keep(&self, app: &Rc<App>, folder: &Path) {
        if !folder.starts_with(&self.vault_root) {
            return;
        }
        folder.clone_into(&mut self.folder.borrow_mut());
        let rel = folder
            .strip_prefix(&self.vault_root)
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.breadcrumb.set_text(&if rel.is_empty() {
            "~".into()
        } else {
            format!("~/{rel}")
        });
        app.vault.list(folder);
    }

    /// F5: read the shown folder from disk again.
    pub fn reload(&self, app: &Rc<App>) {
        *self.want.borrow_mut() = self.cursor_name();
        app.vault.reread(&self.folder());
    }

    pub fn on_listing(&self, _app: &Rc<App>, folder: &Path, entries: &Arc<[Entry]>) {
        if folder != self.folder.borrow().as_path() {
            return;
        }
        *self.listing.borrow_mut() = entries.clone();
        if self.want.borrow().is_none() {
            *self.want.borrow_mut() = self.cursor_name();
        }
        self.rebuild();
    }

    /// Apply visibility and order to the cached listing and swap the rows in one step.
    pub fn rebuild(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let (hidden, only_md) = {
            let c = app.config.borrow();
            (c.files.show_hidden, c.files.only_markdown)
        };
        let (order, reversed) = {
            let s = app.session.borrow();
            (s.sort, s.reversed)
        };
        let mut entries: Vec<Entry> = self
            .listing
            .borrow()
            .iter()
            .filter(|e| sort::visible(e, hidden, only_md))
            .cloned()
            .collect();
        sort::sort_entries(&mut entries, order, reversed);
        let want = self.want.borrow_mut().take();
        let mut cursor = 0;
        let rows: Vec<glib::Object> = entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                if want.as_deref() == Some(entry.name.as_str()) {
                    cursor = i;
                }
                glib::BoxedAnyObject::new(Row::new(entry)).upcast()
            })
            .collect();
        let had_focus = self.has_focus();
        self.visual.borrow_mut().take();
        let removed = self.store.n_items();
        self.store.splice(0, removed, &rows);
        if !rows.is_empty() {
            let pos = u32::try_from(cursor).unwrap_or(0);
            self.selection.set_selected(pos);
            let flags = if had_focus {
                gtk::ListScrollFlags::FOCUS
            } else {
                gtk::ListScrollFlags::NONE
            };
            self.list.scroll_to(pos, flags, None);
        } else if had_focus {
            self.list.grab_focus();
        }
    }

    pub fn has_focus(&self) -> bool {
        self.root
            .root()
            .and_then(|r| r.focus())
            .is_some_and(|f| f.is_ancestor(&self.root))
    }

    pub fn cursor(&self) -> Option<u32> {
        let pos = self.selection.selected();
        (pos != gtk::INVALID_LIST_POSITION).then_some(pos)
    }

    pub fn cursor_entry(&self) -> Option<Entry> {
        let obj = self.selection.selected_item()?;
        row(&obj).map(|r| r.entry.clone())
    }

    pub fn cursor_name(&self) -> Option<String> {
        self.cursor_entry().map(|e| e.name)
    }

    fn place(&self, pos: u32) {
        self.selection.set_selected(pos);
        self.list.scroll_to(pos, gtk::ListScrollFlags::FOCUS, None);
        self.extend_visual();
    }

    fn rows(&self) -> impl Iterator<Item = glib::Object> + '_ {
        (0..self.store.n_items()).filter_map(|i| self.store.item(i))
    }

    fn set_mark(&self, pos: u32, on: bool) {
        if let Some(obj) = self.store.item(pos)
            && let Some(r) = row(&obj)
            && r.marked.replace(on) != on
        {
            r.paint();
        }
    }

    pub fn marked(&self) -> Vec<Entry> {
        self.rows()
            .filter_map(|o| row(&o).filter(|r| r.marked.get()).map(|r| r.entry.clone()))
            .collect()
    }

    /// `Space`: toggle the row under the cursor and step down.
    pub fn toggle_mark(&self, app: &Rc<App>) {
        let Some(pos) = self.cursor() else { return };
        let on = self
            .store
            .item(pos)
            .and_then(|o| row(&o).map(|r| !r.marked.get()))
            .unwrap_or(false);
        self.set_mark(pos, on);
        self.step(app, 1, false);
    }

    /// `v`: start visual mode at the cursor, or leave it keeping the marks.
    pub fn visual_mode(&self, app: &Rc<App>) {
        if self.visual.borrow_mut().take().is_some() {
            app.status.set_mode("");
            return;
        }
        let Some(anchor) = self.cursor() else { return };
        let base = self
            .rows()
            .map(|o| row(&o).is_some_and(|r| r.marked.get()))
            .collect();
        *self.visual.borrow_mut() = Some((anchor, base));
        app.status.set_mode("-- VISUAL --");
        self.extend_visual();
    }

    fn extend_visual(&self) {
        let (anchor, base) = match self.visual.borrow().as_ref() {
            Some((a, b)) => (*a, b.clone()),
            None => return,
        };
        let Some(cursor) = self.cursor() else { return };
        let (lo, hi) = (anchor.min(cursor), anchor.max(cursor));
        for (i, was) in base.iter().enumerate() {
            let pos = u32::try_from(i).unwrap_or(u32::MAX);
            self.set_mark(pos, *was || (lo..=hi).contains(&pos));
        }
    }

    /// `Ctrl+A`.
    pub fn mark_all(&self) {
        for pos in 0..self.store.n_items() {
            self.set_mark(pos, true);
        }
    }

    /// `Ctrl+R`.
    pub fn invert_marks(&self) {
        for pos in 0..self.store.n_items() {
            let on = self
                .store
                .item(pos)
                .and_then(|o| row(&o).map(|r| !r.marked.get()))
                .unwrap_or(false);
            self.set_mark(pos, on);
        }
    }

    /// `Esc`: leave visual mode and clear the selection.
    pub fn clear_marks(&self, app: &Rc<App>) {
        if self.visual.borrow_mut().take().is_some() {
            app.status.set_mode("");
        }
        for pos in 0..self.store.n_items() {
            self.set_mark(pos, false);
        }
    }

    /// Move by `delta` rows; single steps wrap around.
    pub fn step(&self, _app: &Rc<App>, delta: i64, wrap: bool) {
        let n = i64::from(self.store.n_items());
        if n == 0 {
            return;
        }
        let at = self.cursor().map_or(-1, i64::from);
        let next = if wrap {
            (at + delta).rem_euclid(n)
        } else {
            (at + delta).clamp(0, n - 1)
        };
        self.place(u32::try_from(next).unwrap_or(0));
    }

    pub fn jump(&self, _app: &Rc<App>, pos: u32) {
        let n = self.store.n_items();
        if n > 0 {
            self.place(pos.min(n - 1));
        }
    }

    /// Move by a fraction of the visible rows.
    pub fn page(&self, app: &Rc<App>, pages: f64) {
        let n = self.store.n_items();
        if n == 0 {
            return;
        }
        let adj = self.scroller.vadjustment();
        let row_height = (adj.upper() / f64::from(n)).max(1.0);
        let rows = i64::from(crate::app::px(adj.page_size() / row_height * pages));
        let rows = if rows == 0 {
            i64::from(crate::app::px(pages.signum()))
        } else {
            rows
        };
        self.step(app, rows, false);
    }

    pub fn parent(&self, app: &Rc<App>) {
        let folder = self.folder();
        if folder == self.vault_root {
            return;
        }
        if let Some(parent) = folder.parent() {
            *self.want.borrow_mut() = folder.file_name().map(|n| n.to_string_lossy().into_owned());
            self.show_keep(app, parent);
        }
    }

    /// Enter the folder under the cursor, or open the file.
    pub fn open(&self, app: &Rc<App>) {
        let Some(entry) = self.cursor_entry() else {
            return;
        };
        if entry.directory {
            self.show(app, &entry.path);
        } else if entry.name.to_ascii_lowercase().ends_with(".md") {
            app.open_note(&entry.path, false, None);
        } else {
            app.status.notice(&format!("{} is not a note", entry.name));
        }
    }

    fn key(
        &self,
        app: &Rc<App>,
        keyval: gtk::gdk::Key,
        state: gtk::gdk::ModifierType,
    ) -> glib::Propagation {
        // An entry inside the column (find, filter) gets every key.
        let focus = self.root.root().and_then(|r| r.focus());
        if focus.is_some_and(|f| f.is::<gtk::Text>()) {
            return glib::Propagation::Proceed;
        }
        if matches!(
            keyval,
            gtk::gdk::Key::Shift_L
                | gtk::gdk::Key::Shift_R
                | gtk::gdk::Key::Control_L
                | gtk::gdk::Key::Control_R
                | gtk::gdk::Key::Alt_L
                | gtk::gdk::Key::Alt_R
        ) {
            return glib::Propagation::Proceed;
        }
        // Alt chords and function keys belong to the window.
        if state.contains(gtk::gdk::ModifierType::ALT_MASK) {
            return glib::Propagation::Proceed;
        }
        let Some(name) = registry::key_name(keyval, state) else {
            return glib::Propagation::Proceed;
        };
        let sequence = format!("{}{name}", self.pending.borrow());
        match registry::lookup(registry::Scope::Files, &sequence) {
            registry::Lookup::Exact(entry) => {
                self.set_pending(app, "");
                (entry.run)(app, "");
                glib::Propagation::Stop
            }
            registry::Lookup::Prefix => {
                self.set_pending(app, &sequence);
                glib::Propagation::Stop
            }
            registry::Lookup::None => {
                let had = !self.pending.borrow().is_empty();
                self.set_pending(app, "");
                if had || name.starts_with("<F") {
                    if had {
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                } else if name.len() == 1 || name.starts_with("<C-") {
                    // Unbound plain keys do nothing here (no type-ahead search).
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        }
    }

    fn set_pending(&self, app: &Rc<App>, text: &str) {
        text.clone_into(&mut self.pending.borrow_mut());
        app.status.set_pending(text);
    }
}

/// F9: show or hide the column (focus moves to the editor when hidden).
pub fn toggle(app: &Rc<App>) {
    let visible = !app.files.root.is_visible();
    if !visible && app.files.has_focus() {
        crate::editor::focus(app);
    }
    app.files.root.set_visible(visible);
    app.session.borrow_mut().files_visible = visible;
}

/// Alt+1: focus the column, showing it if hidden. `scroll_to(FOCUS)` only
/// moves the keyboard when the list already has it, so grab first.
pub fn focus(app: &Rc<App>) {
    let files = &app.files;
    files.root.set_visible(true);
    files.list.grab_focus();
    match files.cursor() {
        Some(pos) => files.list.scroll_to(pos, gtk::ListScrollFlags::FOCUS, None),
        None if files.store.n_items() > 0 => files.place(0),
        None => {}
    }
}
