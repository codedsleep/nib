//! The finder (P§13): one widget at the top centre of the window, an entry
//! over at most 14 rows, fed by a `FinderSource` — quick switcher, command
//! palette, `z` finder, tags, vault search, templates.
//!
//! It is an overlay box, not a `GtkPopover`: nothing to map as a new
//! surface, so it shows in the next frame, and no compositor resize can pop
//! it down while rows stream in.
use crate::{app::App, debug, editor, gtk, launch, registry};
use gtk::prelude::*;
use nib_core::{
    NoteId,
    fuzzy::{Candidate, CandidateTarget},
    search::SearchSpec,
};
use nib_vault::{
    SearchEvent, WalkEvent,
    fuzzy::{Matcher, Snapshot},
    search::{SearchHandle, WalkHandle},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Instant,
};

/// Rows a finder shows at most.
const ROWS: i32 = 14;

/// What a row stands for.
#[derive(Clone)]
pub enum Item {
    Candidate(Candidate),
    Create(String),
    Hit { note: NoteId, line: u64 },
    More,
    Info,
}

#[derive(Clone)]
pub struct Row {
    pub label: String,
    pub hint: String,
    pub item: Item,
}

/// A source of finder rows.
pub trait FinderSource {
    fn name(&self) -> &'static str;
    fn placeholder(&self) -> &'static str;
    /// The finder opened with this source.
    fn start(&self, app: &Rc<App>);
    /// The query changed.
    fn query(&self, app: &Rc<App>, text: &str);
    /// `Enter` (or `Shift+Enter`, a new tab) on a row.
    fn accept(&self, app: &Rc<App>, row: &Row, query: &str, new_tab: bool);
}

pub struct Finder {
    pub root: gtk::Box,
    entry: gtk::Entry,
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    list: gtk::ListView,
    hint: gtk::Label,
    source: RefCell<Option<Rc<dyn FinderSource>>>,
    /// The matcher of the open source and the query generation it waits for.
    matcher: RefCell<Option<Matcher>>,
    wanted: Rc<Cell<u64>>,
    listener: RefCell<Option<glib::JoinHandle<()>>>,
    search: RefCell<Option<SearchHandle>>,
    walk: RefCell<Option<(WalkHandle, Vec<String>)>>,
    previous_focus: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    typed: Cell<Option<Instant>>,
}

fn boxed(obj: &glib::Object) -> Option<Row> {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .and_then(|b| b.try_borrow::<Row>().ok().map(|r| r.clone()))
}

impl Finder {
    pub fn new(app: &std::rc::Weak<App>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.add_css_class("nib-finder");
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::Start);
        root.set_margin_top(36);
        root.set_visible(false);
        let entry = gtk::Entry::new();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(true);
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let hint = gtk::Label::new(None);
            hint.add_css_class("hint");
            hint.set_ellipsize(gtk::pango::EllipsizeMode::Start);
            hint.set_max_width_chars(40);
            line.append(&label);
            line.append(&hint);
            item.set_child(Some(&line));
            item.set_focusable(false);
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let (Some(line), Some(obj)) = (item.child(), item.item()) else {
                return;
            };
            let Some(row) = boxed(&obj) else { return };
            if let Some(label) = line.first_child().and_downcast::<gtk::Label>() {
                label.set_text(&row.label);
                if matches!(row.item, Item::Info) {
                    label.add_css_class("dim");
                } else {
                    label.remove_css_class("dim");
                }
            }
            if let Some(hint) = line.last_child().and_downcast::<gtk::Label>() {
                hint.set_text(&row.hint);
            }
        });
        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        crate::app::click_activates(&list);
        list.set_can_focus(false);
        list.set_focusable(false);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(ROWS * 26)
            .build();
        let hint = gtk::Label::new(None);
        hint.add_css_class("dim");
        hint.set_xalign(0.0);
        root.append(&entry);
        root.append(&scroller);
        root.append(&hint);
        let finder = Self {
            root,
            entry,
            store,
            selection,
            list,
            hint,
            source: RefCell::new(None),
            matcher: RefCell::new(None),
            wanted: Rc::new(Cell::new(0)),
            listener: RefCell::new(None),
            search: RefCell::new(None),
            walk: RefCell::new(None),
            previous_focus: RefCell::new(None),
            typed: Cell::new(None),
        };
        finder.connect(app);
        finder
    }

    fn connect(&self, app: &std::rc::Weak<App>) {
        let weak = app.clone();
        self.entry.connect_changed(move |e| {
            let Some(app) = weak.upgrade() else { return };
            let source = app.finder.source.borrow().clone();
            if let Some(source) = source {
                app.finder.typed.set(Some(Instant::now()));
                source.query(&app, &e.text());
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = app.clone();
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if app.finder.key(&app, key, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.entry.add_controller(keys);
        let weak = app.clone();
        self.list.connect_activate(move |_, pos| {
            if let Some(app) = weak.upgrade() {
                app.finder.selection.set_selected(pos);
                app.finder.accept(&app, false);
            }
        });
    }

    pub fn visible(&self) -> bool {
        self.root.is_visible()
    }

    pub fn source_name(&self) -> Option<&'static str> {
        self.source.borrow().as_ref().map(|s| s.name())
    }

    pub fn query_text(&self) -> String {
        self.entry.text().into()
    }

    /// Labels of the rows, for `--test-dump`.
    pub fn rows(&self) -> Vec<String> {
        (0..self.store.n_items())
            .filter_map(|i| self.store.item(i))
            .filter_map(|o| boxed(&o).map(|r| r.label))
            .collect()
    }

    /// Show the finder with a source and an initial query.
    pub fn open(&self, app: &Rc<App>, source: &Rc<dyn FinderSource>, query: &str) {
        let started = Instant::now();
        let restore =
            gtk::prelude::GtkWindowExt::focus(&app.window).filter(|f| !f.is_ancestor(&self.root));
        if !self.visible() {
            *self.previous_focus.borrow_mut() = restore.map(|w| w.downgrade());
        }
        self.reset();
        app.completion.close();
        self.entry.set_placeholder_text(Some(source.placeholder()));
        *self.source.borrow_mut() = Some(Rc::clone(source));
        let width = app.window.width().max(640);
        self.root.set_size_request(width * 3 / 5, -1);
        self.root.set_visible(true);
        self.entry.grab_focus();
        source.start(app);
        // Setting the text runs the source's query.
        self.entry.set_text(query);
        if query.is_empty() {
            source.query(app, "");
        }
        self.entry.set_position(-1);
        debug::trace_paint(&self.root, "finder_visible", started);
    }

    fn reset(&self) {
        if let Some(l) = self.listener.borrow_mut().take() {
            l.abort();
        }
        self.matcher.borrow_mut().take();
        if let Some(s) = self.search.borrow_mut().take() {
            s.cancel();
        }
        if let Some((w, _)) = self.walk.borrow_mut().take() {
            w.cancel();
        }
        self.source.borrow_mut().take();
        self.store.remove_all();
        self.hint.set_text("");
    }

    pub fn close(&self) {
        if !self.visible() {
            return;
        }
        self.reset();
        self.root.set_visible(false);
        let previous = self
            .previous_focus
            .borrow_mut()
            .take()
            .and_then(|w| w.upgrade());
        if let Some(w) = previous {
            w.grab_focus();
        }
    }

    fn set_rows(&self, rows: Vec<Row>) {
        let objects: Vec<glib::Object> = rows
            .into_iter()
            .map(|r| glib::BoxedAnyObject::new(r).upcast())
            .collect();
        self.store.splice(0, self.store.n_items(), &objects);
        if self.store.n_items() > 0 {
            self.selection.set_selected(0);
            self.list.scroll_to(0, gtk::ListScrollFlags::NONE, None);
        }
        if let Some(t) = self.typed.take() {
            debug::trace_ms("finder_rows", t);
        }
    }

    fn append_rows(&self, rows: Vec<Row>) {
        let objects: Vec<glib::Object> = rows
            .into_iter()
            .map(|r| glib::BoxedAnyObject::new(r).upcast())
            .collect();
        let at = self.store.n_items();
        self.store.splice(at, 0, &objects);
        if at == 0 && self.store.n_items() > 0 {
            self.selection.set_selected(0);
        }
        if let Some(t) = self.typed.take() {
            debug::trace_ms("finder_rows", t);
        }
    }

    /// Start matching `candidates` (empty-query order kept), mapping each
    /// snapshot through `rows`.
    fn match_over(
        &self,
        app: &Rc<App>,
        candidates: Vec<Candidate>,
        rows: impl Fn(&App, &Snapshot, &str) -> Vec<Row> + 'static,
    ) {
        let matcher = match Matcher::new(candidates) {
            Ok(m) => m,
            Err(e) => {
                self.hint.set_text(&format!("cannot match: {e}"));
                return;
            }
        };
        let events = matcher.events();
        let wanted = self.wanted.clone();
        let weak = app.this.clone();
        *self.listener.borrow_mut() = Some(glib::spawn_future_local(async move {
            while let Ok(snapshot) = events.recv().await {
                let Some(app) = weak.upgrade() else { break };
                if snapshot.query != wanted.get() {
                    continue;
                }
                let query = app.finder.query_text();
                let rows = rows(&app, &snapshot, &query);
                app.finder.set_rows(rows);
                app.finder.hint.set_text(&if snapshot.running {
                    "matching…".to_owned()
                } else {
                    format!(
                        "{} matches",
                        crate::status::group(snapshot.matched as usize)
                    )
                });
            }
        }));
        *self.matcher.borrow_mut() = Some(matcher);
    }

    fn set_query(&self, text: &str) {
        if let Some(m) = self.matcher.borrow().as_ref() {
            self.wanted.set(m.set_query(text));
        }
    }

    fn key(&self, app: &Rc<App>, key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> bool {
        use gtk::gdk::{Key, ModifierType as M};
        let ctrl = state.contains(M::CONTROL_MASK);
        let shift = state.contains(M::SHIFT_MASK);
        match key {
            Key::Escape => self.close(),
            Key::Down | Key::Tab => self.step(1),
            Key::Up | Key::ISO_Left_Tab => self.step(-1),
            Key::n if ctrl => self.step(1),
            Key::p if ctrl => self.step(-1),
            Key::Return | Key::KP_Enter => self.accept(app, shift),
            _ => return false,
        }
        true
    }

    fn step(&self, delta: i64) {
        let n = i64::from(self.store.n_items());
        if n == 0 {
            return;
        }
        let at = self.selection.selected();
        let at = if at == gtk::INVALID_LIST_POSITION {
            -1
        } else {
            i64::from(at)
        };
        let next = u32::try_from((at + delta).rem_euclid(n)).unwrap_or(0);
        self.selection.set_selected(next);
        self.list.scroll_to(next, gtk::ListScrollFlags::NONE, None);
    }

    fn accept(&self, app: &Rc<App>, new_tab: bool) {
        let (Some(source), Some(row)) = (
            self.source.borrow().clone(),
            self.selection.selected_item().and_then(|o| boxed(&o)),
        ) else {
            return;
        };
        let query = self.query_text();
        if matches!(row.item, Item::Info) {
            return;
        }
        self.close();
        source.accept(app, &row, &query, new_tab);
    }

    pub fn on_search(&self, app: &Rc<App>, event: SearchEvent) {
        let current = self.search.borrow().as_ref().map(SearchHandle::id);
        let id = match &event {
            SearchEvent::Batch { id, .. }
            | SearchEvent::More(id)
            | SearchEvent::Done(id)
            | SearchEvent::Failed(id, _) => *id,
        };
        if current != Some(id) {
            return;
        }
        match event {
            SearchEvent::Batch { hits, .. } => {
                let mut rows = Vec::with_capacity(hits.len() + 4);
                let mut last = self.last_note();
                for hit in hits {
                    if last.as_ref() != Some(&hit.note) {
                        let title = app
                            .index
                            .borrow()
                            .notes
                            .get(&hit.note)
                            .map_or_else(|| hit.note.to_string(), |m| m.title.clone());
                        rows.push(Row {
                            label: title,
                            hint: hit.note.filename(),
                            item: Item::Hit {
                                note: hit.note.clone(),
                                line: 1,
                            },
                        });
                        last = Some(hit.note.clone());
                    }
                    rows.push(Row {
                        label: format!("    {}", hit.text.trim()),
                        hint: format!(":{}", hit.line),
                        item: Item::Hit {
                            note: hit.note,
                            line: hit.line,
                        },
                    });
                }
                self.append_rows(rows);
                self.hint.set_text("searching…");
            }
            SearchEvent::More(_) => {
                self.append_rows(vec![Row {
                    label: "more…".into(),
                    hint: "the first 2 000 matches are shown".into(),
                    item: Item::More,
                }]);
            }
            SearchEvent::Done(_) => {
                let n = self.rows().iter().filter(|r| r.starts_with("    ")).count();
                self.hint
                    .set_text(&format!("{} matching lines", crate::status::group(n)));
                if n == 0 {
                    self.set_rows(vec![info("no matches")]);
                }
            }
            SearchEvent::Failed(_, message) => {
                self.set_rows(vec![info(&message)]);
            }
        }
    }

    fn last_note(&self) -> Option<NoteId> {
        let n = self.store.n_items();
        let row = self.store.item(n.checked_sub(1)?).and_then(|o| boxed(&o))?;
        match row.item {
            Item::Hit { note, .. } => Some(note),
            _ => None,
        }
    }

    pub fn on_walk(&self, app: &Rc<App>, event: WalkEvent) {
        let current = self.walk.borrow().as_ref().map(|(w, _)| w.id());
        match event {
            WalkEvent::Batch { id, paths } if Some(id) == current => {
                if let Some((_, all)) = self.walk.borrow_mut().as_mut() {
                    all.extend(paths);
                }
            }
            WalkEvent::Done(id) if Some(id) == current => {
                let paths = self
                    .walk
                    .borrow_mut()
                    .as_mut()
                    .map(|(_, p)| std::mem::take(p))
                    .unwrap_or_default();
                let candidates = paths
                    .into_iter()
                    .map(|p| Candidate {
                        label: p.clone(),
                        text: p.clone(),
                        target: CandidateTarget::Attachment(p),
                    })
                    .collect();
                self.match_over(app, candidates, |_, snapshot, _| {
                    snapshot
                        .candidates
                        .iter()
                        .map(|c| Row {
                            label: c.label.clone(),
                            hint: String::new(),
                            item: Item::Candidate(c.clone()),
                        })
                        .collect()
                });
                let query = self.query_text();
                self.set_query(&query);
            }
            _ => {}
        }
    }
}

fn info(text: &str) -> Row {
    Row {
        label: text.to_owned(),
        hint: String::new(),
        item: Item::Info,
    }
}

fn open_candidate(app: &Rc<App>, target: &CandidateTarget, new_tab: bool) {
    match target {
        CandidateTarget::Note(id) => {
            let path = app.vault.root().join(id.filename());
            app.open_note(&path, new_tab, None);
        }
        CandidateTarget::Heading(id, slug) => {
            let path = app.vault.root().join(id.filename());
            app.open_note(&path, new_tab, None);
            let line = app
                .index
                .borrow()
                .notes
                .get(id)
                .and_then(|m| m.headings.iter().find(|h| &h.slug == slug).map(|h| h.line));
            if let Some(line) = line {
                app.tabs.current().goto_line(line);
            }
        }
        CandidateTarget::Tag(tag) => open(app, TagNotes(tag.clone()), ""),
        CandidateTarget::Attachment(rel) => {
            launch::open_attachment(app, &app.vault.root().join(rel));
        }
    }
}

/// Open the finder with a source.
pub fn open(app: &Rc<App>, source: impl FinderSource + 'static, query: &str) {
    let source: Rc<dyn FinderSource> = Rc::new(source);
    app.finder.open(app, &source, query);
}

// ---- quick switcher -------------------------------------------------------

/// `Alt+O`, `:o`: notes by title, alias and path, recent first, with a
/// `Create "query"` row when nothing matches exactly.
pub struct Switcher;

impl FinderSource for Switcher {
    fn name(&self) -> &'static str {
        "switcher"
    }
    fn placeholder(&self) -> &'static str {
        "open a note"
    }
    fn start(&self, app: &Rc<App>) {
        if !app.index_ready.get() {
            app.finder.set_rows(vec![info("index not ready")]);
            return;
        }
        let candidates = app.switcher_candidates();
        app.finder
            .match_over(app, candidates, |app, snapshot, query| {
                let mut rows: Vec<Row> = snapshot
                    .candidates
                    .iter()
                    .map(|c| Row {
                        label: c.label.clone(),
                        hint: match &c.target {
                            CandidateTarget::Note(id) => id.filename(),
                            _ => String::new(),
                        },
                        item: Item::Candidate(c.clone()),
                    })
                    .collect();
                let query = query.trim();
                if !query.is_empty() && !exact(app, query) {
                    rows.insert(
                        0,
                        Row {
                            label: format!("Create \"{query}\""),
                            hint: launch::new_note_id(app, query)
                                .map_or_else(String::new, |id| id.filename()),
                            item: Item::Create(query.to_owned()),
                        },
                    );
                }
                rows
            });
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        app.finder.set_query(text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, new_tab: bool) {
        match &row.item {
            Item::Candidate(c) => open_candidate(app, &c.target, new_tab),
            Item::Create(name) => create_note(app, name, new_tab),
            _ => {}
        }
    }
}

/// A note whose title, alias, stem or path equals the query.
fn exact(app: &App, query: &str) -> bool {
    let q = query.to_lowercase();
    let index = app.index.borrow();
    index.by_stem.contains_key(&q)
        || index.by_alias.contains_key(&q)
        || index.by_path.contains_key(&q)
        || index.notes.values().any(|m| m.title.to_lowercase() == q)
}

/// Create a note (never over an existing one) and open it.
pub fn create_note(app: &Rc<App>, name: &str, new_tab: bool) {
    let Some(id) = launch::new_note_id(app, name) else {
        app.status
            .error(&format!("cannot make a note called {name}"));
        return;
    };
    match app.vault.create_note(&id, "") {
        Ok(path) => {
            app.open_note(&path, new_tab, None);
            app.status.notice(&format!("created {}", id.filename()));
        }
        Err(e) => app
            .status
            .error(&format!("cannot create {}: {e}", id.filename())),
    }
}

// ---- command palette ------------------------------------------------------

/// `Alt+P`: every registry entry, fuzzy, with its keys as a hint.
pub struct Palette;

impl FinderSource for Palette {
    fn name(&self) -> &'static str {
        "palette"
    }
    fn placeholder(&self) -> &'static str {
        "run a command"
    }
    fn start(&self, app: &Rc<App>) {
        // The registry name rides in the Tag slot of a candidate.
        let candidates = registry::ENTRIES
            .iter()
            .filter(|e| e.scope != registry::Scope::Files)
            .map(|e| Candidate {
                label: e.label.to_owned(),
                text: format!("{} {} {}", e.label, e.name, e.commands.join(" ")),
                target: CandidateTarget::Tag(e.name.to_owned()),
            })
            .collect();
        app.finder.match_over(app, candidates, |_, snapshot, _| {
            snapshot
                .candidates
                .iter()
                .filter_map(|c| {
                    let CandidateTarget::Tag(name) = &c.target else {
                        return None;
                    };
                    let e = registry::find(name)?;
                    let keys: Vec<String> = e
                        .keys
                        .iter()
                        .map(|k| registry::display_key(e.scope, k))
                        .chain(e.commands.first().map(|c| format!(":{c}")))
                        .collect();
                    Some(Row {
                        label: e.label.to_owned(),
                        hint: keys.join("  "),
                        item: Item::Candidate(c.clone()),
                    })
                })
                .collect()
        });
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        app.finder.set_query(text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, _: bool) {
        if let Item::Candidate(Candidate {
            target: CandidateTarget::Tag(name),
            ..
        }) = &row.item
        {
            registry::run(app, name, "");
        }
    }
}

// ---- z finder ---------------------------------------------------------------

/// `z`: everything below the files column's folder.
pub struct Walk(pub PathBuf);

impl FinderSource for Walk {
    fn name(&self) -> &'static str {
        "walk"
    }
    fn placeholder(&self) -> &'static str {
        "find below this folder"
    }
    fn start(&self, app: &Rc<App>) {
        let handle = app.vault.walk_below(&self.0);
        *app.finder.walk.borrow_mut() = Some((handle, Vec::new()));
        app.finder.hint.set_text("walking…");
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        app.finder.set_query(text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, new_tab: bool) {
        let Item::Candidate(Candidate {
            target: CandidateTarget::Attachment(rel),
            ..
        }) = &row.item
        else {
            return;
        };
        let path = self.0.join(rel.trim_end_matches('/'));
        if rel.ends_with('/') {
            app.files.show(app, &path);
            crate::files::focus(app);
        } else if rel.to_ascii_lowercase().ends_with(".md") {
            app.open_note(&path, new_tab, None);
        } else {
            launch::open_attachment(app, &path);
        }
    }
}

// ---- tags -------------------------------------------------------------------

/// `:tag`: tags with their note counts; choosing one lists its notes.
pub struct Tags;

impl FinderSource for Tags {
    fn name(&self) -> &'static str {
        "tags"
    }
    fn placeholder(&self) -> &'static str {
        "tag"
    }
    fn start(&self, app: &Rc<App>) {
        if !app.index_ready.get() {
            app.finder.set_rows(vec![info("index not ready")]);
            return;
        }
        let candidates = nib_core::fuzzy::tag_candidates(&app.index.borrow());
        app.finder.match_over(app, candidates, candidate_rows);
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        app.finder.set_query(text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, new_tab: bool) {
        if let Item::Candidate(c) = &row.item {
            open_candidate(app, &c.target, new_tab);
        }
    }
}

fn candidate_rows(_: &App, snapshot: &Snapshot, _: &str) -> Vec<Row> {
    snapshot
        .candidates
        .iter()
        .map(|c| Row {
            label: c.label.clone(),
            hint: match &c.target {
                CandidateTarget::Note(id) => id.filename(),
                _ => String::new(),
            },
            item: Item::Candidate(c.clone()),
        })
        .collect()
}

/// The notes of one tag.
pub struct TagNotes(pub String);

impl FinderSource for TagNotes {
    fn name(&self) -> &'static str {
        "tag-notes"
    }
    fn placeholder(&self) -> &'static str {
        "note with this tag"
    }
    fn start(&self, app: &Rc<App>) {
        let tag = nib_core::Tag::new(self.0.trim_start_matches('#'));
        let candidates: Vec<Candidate> = {
            let index = app.index.borrow();
            index
                .tags
                .get(&tag)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| index.notes.get(id))
                        .map(|m| Candidate {
                            label: m.title.clone(),
                            text: format!("{} {}", m.title, m.id),
                            target: CandidateTarget::Note(m.id.clone()),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        if candidates.is_empty() {
            app.finder
                .set_rows(vec![info(&format!("no notes tagged #{}", tag.0))]);
            return;
        }
        app.finder.match_over(app, candidates, candidate_rows);
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        app.finder.set_query(text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, new_tab: bool) {
        if let Item::Candidate(c) = &row.item {
            open_candidate(app, &c.target, new_tab);
        }
    }
}

// ---- vault search -------------------------------------------------------------

/// `Alt+F`, `:find`: in-process grep over the vault, streamed and grouped
/// by note; a new query cancels the running search.
pub struct Search;

impl FinderSource for Search {
    fn name(&self) -> &'static str {
        "search"
    }
    fn placeholder(&self) -> &'static str {
        "search the vault (r: regex, -word excludes paths)"
    }
    fn start(&self, app: &Rc<App>) {
        // Search reads the files: save edits first (P§16).
        for tab in app.tabs.all() {
            editor::save_if_modified(app, &tab);
        }
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        if let Some(old) = app.finder.search.borrow_mut().take() {
            old.cancel();
        }
        app.finder.store.remove_all();
        if text.trim().is_empty() {
            app.finder.hint.set_text("");
            return;
        }
        let handle = app.vault.search(SearchSpec::parse(text));
        *app.finder.search.borrow_mut() = Some(handle);
        app.finder.hint.set_text("searching…");
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, new_tab: bool) {
        if let Item::Hit { note, line } = &row.item {
            let path = app.vault.root().join(note.filename());
            app.open_note(&path, new_tab, Some(u32::try_from(*line).unwrap_or(1)));
        }
    }
}
