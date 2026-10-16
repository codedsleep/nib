//! The files column (P§8): one virtualised `ListView` over the listing the
//! vault's listing thread delivers, driven with vim motions. The cursor is
//! not the selection; commands act on the selection, else the cursor row.
use crate::{app::App, gtk, registry};
use gtk::prelude::*;
use nib_core::{
    settings::{Linemode, Sort},
    sort::{self, Entry},
};
use nib_vault::state::Session;
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
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

pub fn row(obj: &glib::Object) -> Option<std::cell::Ref<'_, Row>> {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .and_then(|b| b.try_borrow::<Row>().ok())
}

/// What the entry at the bottom of the column is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prompt {
    None,
    Find { backwards: bool },
    Filter,
}

/// A list of rows over a store, as the files and parent columns use it.
fn column(app: &Weak<App>, ages: bool) -> (gio::ListStore, gtk::SingleSelection, gtk::ListView) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let factory = gtk::SignalListItemFactory::new();
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
    let weak = app.clone();
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
            let show = ages && weak.upgrade().is_some_and(|a| a.files.linemode.get());
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
    (store, selection, list)
}

fn scrolled(list: &gtk::ListView) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(list)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build()
}

pub struct Files {
    pub root: gtk::Box,
    breadcrumb: gtk::Label,
    pub store: gio::ListStore,
    pub selection: gtk::SingleSelection,
    pub list: gtk::ListView,
    scroller: gtk::ScrolledWindow,
    /// F7: the parent folder, one part wide to the column's three.
    parent_box: gtk::ScrolledWindow,
    parent_store: gio::ListStore,
    parent_selection: gtk::SingleSelection,
    entry: gtk::Entry,
    prompt: Cell<Prompt>,
    vault_root: PathBuf,
    folder: RefCell<PathBuf>,
    listing: RefCell<Arc<[Entry]>>,
    pending: RefCell<String>,
    want: RefCell<Option<String>>,
    /// Visual mode: its anchor row and the marks from before it started.
    visual: RefCell<Option<(u32, Vec<bool>)>>,
    filter: RefCell<String>,
    find: RefCell<Option<(String, bool)>>,
    cut: RefCell<HashSet<PathBuf>>,
    hidden: Cell<bool>,
    /// Folder history for `H`/`L`: (folder, cursor name).
    back: RefCell<Vec<(PathBuf, Option<String>)>>,
    forward: RefCell<Vec<(PathBuf, Option<String>)>>,
    pub linemode: Cell<bool>,
    app: Weak<App>,
}

impl Files {
    pub fn new(
        app: &Weak<App>,
        vault_root: &Path,
        session: &Session,
        config: &nib_core::settings::Config,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("nib-files");
        root.set_width_request(120);
        let breadcrumb = gtk::Label::new(Some("~"));
        breadcrumb.add_css_class("nib-breadcrumb");
        breadcrumb.set_xalign(0.0);
        breadcrumb.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        let (store, selection, list) = column(app, true);
        let scroller = scrolled(&list);
        let (parent_store, parent_selection, parent_list) = column(app, false);
        parent_list.set_can_focus(false);
        parent_list.set_focusable(false);
        let parent_box = scrolled(&parent_list);
        parent_box.add_css_class("nib-parent");
        parent_box.set_hexpand(false);
        parent_box.set_visible(false);
        let lists = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        lists.append(&parent_box);
        lists.append(&scroller);
        let entry = gtk::Entry::new();
        entry.set_visible(false);
        root.append(&breadcrumb);
        root.append(&lists);
        root.append(&entry);
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
            parent_box,
            parent_store,
            parent_selection,
            entry,
            prompt: Cell::new(Prompt::None),
            vault_root: vault_root.to_owned(),
            folder: RefCell::new(folder),
            listing: RefCell::new(Arc::from(Vec::new())),
            pending: RefCell::new(String::new()),
            want: RefCell::new((!session.cursor.is_empty()).then(|| session.cursor.clone())),
            visual: RefCell::new(None),
            filter: RefCell::new(String::new()),
            find: RefCell::new(None),
            cut: RefCell::new(HashSet::new()),
            hidden: Cell::new(config.files.show_hidden),
            back: RefCell::new(Vec::new()),
            forward: RefCell::new(Vec::new()),
            linemode: Cell::new(
                config.files.linemode == Linemode::Modified || session.sort == Sort::Modified,
            ),
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
        let weak = self.app.clone();
        self.entry.connect_changed(move |e| {
            if let Some(app) = weak.upgrade() {
                app.files.prompt_changed(&e.text());
            }
        });
        let weak = self.app.clone();
        self.entry.connect_activate(move |_| {
            if let Some(app) = weak.upgrade() {
                app.files.close_prompt(false);
            }
        });
        let entry_keys = gtk::EventControllerKey::new();
        let weak = self.app.clone();
        entry_keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape
                && let Some(app) = weak.upgrade()
            {
                app.files.close_prompt(true);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.entry.add_controller(entry_keys);
    }

    /// Post-present start: list the session's folder, or the folder of a
    /// note named on the command line.
    pub fn start(&self, app: &Rc<App>, follow_note: bool) {
        let tab = app.tabs.current();
        let note = tab.path.borrow().clone().filter(|_| follow_note);
        if let Some(note) = note
            && let Some(parent) = note.parent()
        {
            *self.want.borrow_mut() = note.file_name().map(|n| n.to_string_lossy().into_owned());
            parent.clone_into(&mut self.folder.borrow_mut());
        }
        if app.session.borrow().parent_visible {
            self.parent_box.set_visible(true);
            self.parent_box.set_size_request(60, -1);
        }
        let folder = self.folder.borrow().clone();
        self.go(app, &folder);
    }

    pub fn folder(&self) -> PathBuf {
        self.folder.borrow().clone()
    }

    /// Put the cursor on `name` when the folder is next listed.
    pub fn want_cursor(&self, name: &str) {
        *self.want.borrow_mut() = Some(name.to_owned());
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
        let current = self.folder();
        if current != folder {
            self.back.borrow_mut().push((current, self.cursor_name()));
            self.forward.borrow_mut().clear();
            self.filter.borrow_mut().clear();
        }
        self.go(app, folder);
    }

    fn go(&self, app: &Rc<App>, folder: &Path) {
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
        self.list_parent(app);
        app.session.borrow_mut().folder = rel;
    }

    fn list_parent(&self, app: &Rc<App>) {
        if !self.parent_box.is_visible() {
            return;
        }
        let folder = self.folder();
        match folder.parent().filter(|_| folder != self.vault_root) {
            Some(parent) => app.vault.list(parent),
            None => self.parent_store.remove_all(),
        }
    }

    /// `H` / `L`: back and forward through the folders shown.
    pub fn history(&self, app: &Rc<App>, back: bool) {
        let (from, to) = if back {
            (&self.back, &self.forward)
        } else {
            (&self.forward, &self.back)
        };
        let Some((folder, cursor)) = from.borrow_mut().pop() else {
            return;
        };
        to.borrow_mut().push((self.folder(), self.cursor_name()));
        *self.want.borrow_mut() = cursor;
        self.filter.borrow_mut().clear();
        self.go(app, &folder);
    }

    /// F5: read the shown folder from disk again.
    pub fn reload(&self, app: &Rc<App>) {
        *self.want.borrow_mut() = self.cursor_name();
        app.vault.reread(&self.folder());
    }

    pub fn on_listing(&self, _app: &Rc<App>, folder: &Path, entries: &Arc<[Entry]>) {
        let current = self.folder();
        if self.parent_box.is_visible()
            && current != self.vault_root
            && current.parent() == Some(folder)
        {
            self.fill_parent(entries, &current);
        }
        if folder != current {
            return;
        }
        *self.listing.borrow_mut() = entries.clone();
        if self.want.borrow().is_none() {
            *self.want.borrow_mut() = self.cursor_name();
        }
        self.rebuild();
    }

    fn visible_entries(&self, entries: &[Entry]) -> Vec<Entry> {
        let Some(app) = self.app.upgrade() else {
            return vec![];
        };
        let only_md = app.config.borrow().files.only_markdown;
        let (order, reversed) = {
            let s = app.session.borrow();
            (s.sort, s.reversed)
        };
        let filter = self.filter.borrow().to_lowercase();
        let mut out: Vec<Entry> = entries
            .iter()
            .filter(|e| sort::visible(e, self.hidden.get(), only_md))
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .cloned()
            .collect();
        sort::sort_entries(&mut out, order, reversed);
        out
    }

    fn fill_parent(&self, entries: &[Entry], current: &Path) {
        let name = current
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let mut at = gtk::INVALID_LIST_POSITION;
        let rows: Vec<glib::Object> = self
            .visible_entries(entries)
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                if Some(&e.name) == name.as_ref() {
                    at = u32::try_from(i).unwrap_or(0);
                }
                glib::BoxedAnyObject::new(Row::new(e)).upcast()
            })
            .collect();
        self.parent_store
            .splice(0, self.parent_store.n_items(), &rows);
        self.parent_selection.set_selected(at);
    }

    /// Apply visibility, filter and order to the cached listing and swap the
    /// rows in one step, keeping marks and the cursor by name.
    pub fn rebuild(&self) {
        let marked: HashSet<String> = self.marked().into_iter().map(|e| e.name).collect();
        let entries = self.visible_entries(&self.listing.borrow());
        let want = self.want.borrow_mut().take();
        let cut = self.cut.borrow();
        let mut cursor = 0;
        let rows: Vec<glib::Object> = entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                if want.as_deref() == Some(entry.name.as_str()) {
                    cursor = i;
                }
                let row = Row::new(entry);
                row.marked.set(marked.contains(&row.entry.name));
                row.cut.set(cut.contains(&row.entry.path));
                glib::BoxedAnyObject::new(row).upcast()
            })
            .collect();
        drop(cut);
        let had_focus = self.has_focus() && !self.entry.has_focus();
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
        if let Some(app) = self.app.upgrade() {
            app.session.borrow_mut().cursor = self.cursor_name().unwrap_or_default();
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
        if let Some(app) = self.app.upgrade() {
            app.session.borrow_mut().cursor = self.cursor_name().unwrap_or_default();
        }
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

    /// The selection, or the row under the cursor when nothing is selected.
    pub fn targets(&self) -> Vec<Entry> {
        let marked = self.marked();
        if marked.is_empty() {
            self.cursor_entry().into_iter().collect()
        } else {
            marked
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

    /// Leave visual mode and clear the selection.
    pub fn clear_marks(&self, app: &Rc<App>) {
        if self.visual.borrow_mut().take().is_some() {
            app.status.set_mode("");
        }
        for pos in 0..self.store.n_items() {
            self.set_mark(pos, false);
        }
    }

    /// `Esc`: clear the selection and the filter.
    pub fn escape(&self, app: &Rc<App>) {
        self.clear_marks(app);
        if !self.filter.borrow().is_empty() {
            self.filter.borrow_mut().clear();
            *self.want.borrow_mut() = self.cursor_name();
            self.rebuild();
        }
    }

    /// Dim the rows a cut put on the clipboard (`&[]` after a yank).
    pub fn mark_cut(&self, paths: &[PathBuf]) {
        *self.cut.borrow_mut() = paths.iter().cloned().collect();
        let cut = self.cut.borrow();
        for o in self.rows() {
            if let Some(r) = row(&o) {
                let on = cut.contains(&r.entry.path);
                if r.cut.replace(on) != on {
                    r.paint();
                }
            }
        }
    }

    pub fn clear_cut(&self) {
        self.mark_cut(&[]);
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
            let name = folder.file_name().map(|n| n.to_string_lossy().into_owned());
            self.show_keep(app, parent);
            *self.want.borrow_mut() = name;
        }
    }

    /// Enter the folder under the cursor, or open the file.
    pub fn open(&self, app: &Rc<App>) {
        let Some(entry) = self.cursor_entry() else {
            return;
        };
        self.open_entry(app, &entry, false);
    }

    /// `O`: a note in a new tab.
    pub fn open_new_tab(&self, app: &Rc<App>) {
        if let Some(entry) = self.cursor_entry() {
            self.open_entry(app, &entry, true);
        }
    }

    fn open_entry(&self, app: &Rc<App>, entry: &Entry, new_tab: bool) {
        if entry.directory {
            self.show(app, &entry.path);
        } else if entry.name.to_ascii_lowercase().ends_with(".md")
            || crate::reading::is_image(&entry.path)
        {
            app.open_note(&entry.path, new_tab, None);
        } else {
            crate::launch::open_attachment(app, &entry.path);
        }
    }

    /// `gh` `gd` `ga` `gt`: a configured folder (relative to the vault).
    pub fn go_to(&self, app: &Rc<App>, rel: &str) {
        let folder = self.vault_root.join(rel.trim_matches('/'));
        if folder.is_dir() {
            self.show(app, &folder);
        } else {
            app.status.notice(&format!("no folder {rel} in this vault"));
        }
    }

    /// `,n` `,m`: sort by name (line mode from config) or by modified time (ages shown).
    pub fn sort_by(&self, app: &Rc<App>, order: Sort) {
        app.session.borrow_mut().sort = order;
        self.linemode.set(
            order == Sort::Modified || app.config.borrow().files.linemode == Linemode::Modified,
        );
        self.relist();
    }

    /// `,r`.
    pub fn reverse(&self, app: &Rc<App>) {
        let mut session = app.session.borrow_mut();
        session.reversed = !session.reversed;
        drop(session);
        self.relist();
    }

    /// `.`.
    pub fn toggle_hidden(&self, app: &Rc<App>) {
        self.hidden.set(!self.hidden.get());
        app.status.notice(if self.hidden.get() {
            "hidden files shown"
        } else {
            "hidden files hidden"
        });
        self.relist();
    }

    fn relist(&self) {
        *self.want.borrow_mut() = self.cursor_name();
        self.rebuild();
    }

    /// F7.
    pub fn toggle_parent(&self, app: &Rc<App>) {
        let show = !self.parent_box.is_visible();
        if show {
            let width = self.root.width().max(200);
            self.parent_box.set_size_request(width / 4, -1);
        }
        self.parent_box.set_visible(show);
        app.session.borrow_mut().parent_visible = show;
        self.list_parent(app);
    }

    pub fn parent_visible(&self) -> bool {
        self.parent_box.is_visible()
    }

    /// `/` `?` find in the list, `f` filter it: the entry at the bottom.
    pub fn open_prompt(&self, find: Option<bool>) {
        self.prompt.set(match find {
            Some(backwards) => Prompt::Find { backwards },
            None => Prompt::Filter,
        });
        let text = if find.is_none() {
            self.filter.borrow().clone()
        } else {
            String::new()
        };
        self.entry.set_text(&text);
        self.entry.set_placeholder_text(Some(match find {
            Some(false) => "find",
            Some(true) => "find backwards",
            None => "filter",
        }));
        self.entry.set_visible(true);
        self.entry.grab_focus();
    }

    fn prompt_changed(&self, text: &str) {
        match self.prompt.get() {
            Prompt::Find { backwards } => {
                if !text.is_empty() {
                    *self.find.borrow_mut() = Some((text.to_lowercase(), backwards));
                    self.find_next(false, true);
                }
            }
            Prompt::Filter => {
                text.clone_into(&mut self.filter.borrow_mut());
                *self.want.borrow_mut() = self.cursor_name();
                self.rebuild();
            }
            Prompt::None => {}
        }
    }

    fn close_prompt(&self, cancel: bool) {
        if cancel && self.prompt.get() == Prompt::Filter {
            self.filter.borrow_mut().clear();
            *self.want.borrow_mut() = self.cursor_name();
            self.rebuild();
        }
        self.prompt.set(Prompt::None);
        self.entry.set_visible(false);
        self.list.grab_focus();
        if let Some(pos) = self.cursor() {
            self.list.scroll_to(pos, gtk::ListScrollFlags::FOCUS, None);
        }
    }

    /// `n` / `N`: the next match of the last find (`reverse` flips its direction).
    /// `from_cursor` includes the cursor row (live find while typing).
    pub fn find_next(&self, reverse: bool, from_cursor: bool) {
        let Some((pattern, backwards)) = self.find.borrow().clone() else {
            return;
        };
        let n = self.store.n_items();
        if n == 0 {
            return;
        }
        let back = backwards != reverse;
        let at = self.cursor().unwrap_or(0);
        let start = u32::from(!from_cursor);
        for k in start..n + start {
            let pos = if back {
                (at + n - (k % n)) % n
            } else {
                (at + k) % n
            };
            let hit = self
                .store
                .item(pos)
                .and_then(|o| row(&o).map(|r| r.entry.name.to_lowercase().contains(&pattern)))
                .unwrap_or(false);
            if hit {
                self.selection.set_selected(pos);
                self.list.scroll_to(pos, gtk::ListScrollFlags::NONE, None);
                return;
            }
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
                if had || name.chars().count() == 1 || name.starts_with("<C-") {
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
