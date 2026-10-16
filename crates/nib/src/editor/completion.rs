//! Link and tag completion (P§9, §12): nib's own popover, opened by
//! `completion_trigger` after a typed character or by `Ctrl+Space`, ranked
//! by the vault's nucleo matcher. The keyboard never leaves the editor.
use super::{Tab, vim};
use crate::{app::App, debug, gtk};
use gtk::prelude::*;
use nib_core::{
    edit::{self, CompletionKind},
    fuzzy::{Candidate, CandidateTarget},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

struct Session {
    tab: u64,
    kind: CompletionKind,
    /// Buffer offset (chars) where the typed query starts.
    start: i32,
    target: Option<String>,
    matcher: Option<nib_vault::fuzzy::Matcher>,
    query: Rc<Cell<u64>>,
    listener: Option<glib::JoinHandle<()>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(l) = self.listener.take() {
            l.abort();
        }
    }
}

pub struct Completion {
    popover: gtk::Popover,
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    hint: gtk::Label,
    session: RefCell<Option<Session>>,
    /// Set while nib itself edits (accepting), so its own insertion does not reopen.
    suppress: Cell<bool>,
}

fn candidate(obj: &glib::Object) -> Option<Candidate> {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .and_then(|b| b.try_borrow::<Candidate>().ok().map(|c| c.clone()))
}

impl Completion {
    pub fn new() -> Self {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(true);
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(60);
            item.set_child(Some(&label));
            item.set_focusable(false);
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let (Some(label), Some(obj)) = (item.child().and_downcast::<gtk::Label>(), item.item())
            else {
                return;
            };
            if let Some(c) = candidate(&obj) {
                label.set_text(&c.label);
            }
        });
        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        list.set_can_focus(false);
        list.set_focusable(false);
        let hint = gtk::Label::new(None);
        hint.add_css_class("dim");
        hint.set_xalign(0.0);
        // A fixed size: GTK pops a popover down when the compositor configures
        // it smaller than its new minimum, which happened as rows arrived.
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
        column.set_size_request(420, 14 * 22);
        column.append(&scroller);
        column.append(&hint);
        let popover = gtk::Popover::builder()
            .autohide(false)
            .has_arrow(false)
            .position(gtk::PositionType::Bottom)
            .can_focus(false)
            .child(&column)
            .build();
        popover.add_css_class("nib-finder");
        popover.add_css_class("nib-completion");
        Self {
            popover,
            store,
            selection,
            hint,
            session: RefCell::new(None),
            suppress: Cell::new(false),
        }
    }

    pub fn visible(&self) -> bool {
        self.popover.is_visible()
    }

    /// Hide the popover. It stays parented to its view: unparenting and
    /// re-parenting in one turn of the main loop left the next popup unshown.
    pub fn close(&self) {
        self.session.borrow_mut().take();
        if self.popover.is_visible() {
            self.popover.popdown();
        }
        self.store.remove_all();
    }

    /// Let go of a view that is about to be destroyed.
    pub fn release(&self, view: &sourceview5::View) {
        if self
            .popover
            .parent()
            .is_some_and(|p| &p == view.upcast_ref::<gtk::Widget>())
        {
            self.close();
            self.popover.unparent();
        }
    }

    /// Rows and hint, for `--test-dump`.
    pub fn rows(&self) -> Vec<String> {
        (0..self.store.n_items())
            .filter_map(|i| self.store.item(i))
            .filter_map(|o| candidate(&o).map(|c| c.label))
            .collect()
    }

    fn show(&self, snapshot: &nib_vault::fuzzy::Snapshot) {
        let rows: Vec<glib::Object> = snapshot
            .candidates
            .iter()
            .map(|c| glib::BoxedAnyObject::new(c.clone()).upcast())
            .collect();
        self.store.splice(0, self.store.n_items(), &rows);
        if !rows.is_empty() {
            self.selection.set_selected(0);
        }
        self.hint.set_visible(rows.is_empty());
        if rows.is_empty() {
            self.hint.set_text(if snapshot.running {
                "matching…"
            } else {
                "no matches"
            });
        }
    }
}

/// After every change in a tab: open, narrow or close the popover.
pub fn typed(app: &Rc<App>, tab: &Rc<Tab>) {
    let completion = &app.completion;
    if completion.suppress.get() {
        return;
    }
    let open = completion.session.borrow().is_some();
    if !app.tabs.is_current(tab) || !vim::inserting(tab) {
        if open {
            completion.close();
        }
        return;
    }
    update(app, tab, false);
}

fn update(app: &Rc<App>, tab: &Rc<Tab>, forced: bool) -> bool {
    let started = Instant::now();
    let completion = &app.completion;
    let cursor = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
    let mut line_start = cursor;
    line_start.set_line_offset(0);
    let before = tab.buffer.text(&line_start, &cursor, true);
    let Some(c) = edit::completion_trigger(&before, before.len()) else {
        if completion.session.borrow().is_some() {
            completion.close();
        }
        debug::trace_us("completion_key", started);
        return false;
    };
    if c.kind == CompletionKind::Tag && c.query.is_empty() && !forced {
        // A lone `#` is usually a heading being typed.
        completion.close();
        return false;
    }
    let start =
        line_start.offset() + i32::try_from(before[..c.range.start].chars().count()).unwrap_or(0);
    let target = c.target.map(str::to_owned);
    let same = completion.session.borrow().as_ref().is_some_and(|s| {
        s.tab == tab.id && s.kind == c.kind && s.start == start && s.target == target
    });
    if same {
        if let Some(s) = completion.session.borrow().as_ref()
            && let Some(m) = &s.matcher
        {
            s.query.set(m.set_query(c.query));
        }
        debug::trace_us("completion_key", started);
    } else {
        open(app, tab, c.kind, start, target, c.query);
        debug::trace_ms("completion_open", started);
    }
    place(app, tab, &cursor);
    true
}

fn open(
    app: &Rc<App>,
    tab: &Rc<Tab>,
    kind: CompletionKind,
    start: i32,
    target: Option<String>,
    query: &str,
) {
    let completion = &app.completion;
    completion.close();
    let mut session = Session {
        tab: tab.id,
        kind,
        start,
        target,
        matcher: None,
        query: Rc::new(Cell::new(0)),
        listener: None,
    };
    if app.index_ready.get() {
        let candidates = nib_core::fuzzy::completion_candidates(
            kind,
            &app.index.borrow(),
            session.target.as_deref().unwrap_or(""),
        );
        match nib_vault::fuzzy::Matcher::new(candidates) {
            Ok(matcher) => {
                session.query.set(matcher.set_query(query));
                let events = matcher.events();
                let wanted = session.query.clone();
                let weak = app.this.clone();
                session.listener = Some(glib::spawn_future_local(async move {
                    while let Ok(snapshot) = events.recv().await {
                        let Some(app) = weak.upgrade() else { break };
                        if snapshot.query == wanted.get() {
                            app.completion.show(&snapshot);
                        }
                    }
                }));
                session.matcher = Some(matcher);
                completion.hint.set_text("matching…");
                completion.hint.set_visible(true);
            }
            Err(e) => {
                completion.hint.set_text(&format!("completion failed: {e}"));
                completion.hint.set_visible(true);
            }
        }
    } else {
        completion.hint.set_text("index not ready");
        completion.hint.set_visible(true);
    }
    *completion.session.borrow_mut() = Some(session);
    let view = tab.view.upcast_ref::<gtk::Widget>();
    if completion.popover.parent().as_ref() != Some(view) {
        if completion.popover.parent().is_some() {
            completion.popover.unparent();
        }
        completion.popover.set_parent(view);
    }
}

/// Point the popover at the cursor.
fn place(app: &App, tab: &Tab, cursor: &gtk::TextIter) {
    let rect = tab.view.iter_location(cursor);
    let (x, y) = tab
        .view
        .buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
    let popover = &app.completion.popover;
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, 1, rect.height())));
    if !popover.is_visible() {
        popover.popup();
    }
}

/// `Ctrl+Space`: open for the link or tag at the cursor.
pub fn force(app: &Rc<App>) {
    let tab = app.tabs.current();
    if !update(app, &tab, true) {
        app.status
            .notice("no link or tag to complete at the cursor");
    }
}

/// Keys while the popover is open; true when consumed.
pub fn key(
    app: &Rc<App>,
    tab: &Rc<Tab>,
    keyval: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    use gtk::gdk::{Key, ModifierType as M};
    let completion = &app.completion;
    if !completion.visible() {
        return false;
    }
    let ctrl = state.contains(M::CONTROL_MASK);
    let n = completion.store.n_items();
    let at = completion.selection.selected();
    let step = |delta: i64| {
        if n > 0 {
            let at = if at == gtk::INVALID_LIST_POSITION {
                -1
            } else {
                i64::from(at)
            };
            let next = (at + delta).rem_euclid(i64::from(n));
            completion
                .selection
                .set_selected(u32::try_from(next).unwrap_or(0));
        }
    };
    match keyval {
        Key::Escape => completion.close(),
        Key::Down => step(1),
        Key::Up => step(-1),
        Key::n if ctrl => step(1),
        Key::p if ctrl => step(-1),
        Key::Tab | Key::Return | Key::KP_Enter if !ctrl => accept(app, tab),
        _ => return false,
    }
    true
}

/// Write the chosen link in the configured format and step past `]]`.
fn accept(app: &Rc<App>, tab: &Rc<Tab>) {
    let completion = &app.completion;
    let chosen = completion
        .selection
        .selected_item()
        .and_then(|o| candidate(&o));
    let Some((kind, start)) = completion
        .session
        .borrow()
        .as_ref()
        .map(|s| (s.kind, s.start))
    else {
        return;
    };
    completion.close();
    let Some(chosen) = chosen else { return };
    let (format, from) = {
        let cfg = app.config.borrow();
        let from = tab.path.borrow().as_deref().and_then(|p| app.note_id(p));
        (cfg.links.format, from)
    };
    let index = app.index.borrow();
    let text = match &chosen.target {
        CandidateTarget::Note(id) => {
            let link = match &from {
                Some(from) => nib_core::links::link_from(&index, from, id, format),
                None => nib_core::shortest_link(&index, id, format),
            };
            match chosen.label.split_once(" → ") {
                Some((alias, _)) => format!("{link}|{alias}"),
                None => link,
            }
        }
        CandidateTarget::Heading(_, _) => chosen.label.clone(),
        CandidateTarget::Tag(t) => t.clone(),
        CandidateTarget::Attachment(path) => {
            nib_core::links::attachment_link(&index, from.as_ref(), path, format)
        }
    };
    drop(index);
    let buffer = &tab.buffer;
    completion.suppress.set(true);
    buffer.begin_user_action();
    let mut from_iter = buffer.iter_at_offset(start);
    let mut to_iter = buffer.iter_at_mark(&buffer.get_insert());
    buffer.delete(&mut from_iter, &mut to_iter);
    let mut at = buffer.iter_at_offset(start);
    buffer.insert(&mut at, &text);
    if kind == CompletionKind::Tag {
        buffer.place_cursor(&at);
    } else {
        let mut after = at;
        let mut close = at;
        close.forward_chars(2);
        if buffer.text(&after, &close, true) == "]]" {
            after = close;
        } else {
            buffer.insert(&mut after, "]]");
        }
        buffer.place_cursor(&after);
    }
    buffer.end_user_action();
    completion.suppress.set(false);
}
