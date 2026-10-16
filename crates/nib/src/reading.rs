//! The reading view (P§11): `nib-core::render` operations applied to a plain
//! `GtkTextView` over one tag table shared by every tab and coloured from
//! the active `GtkSourceView` scheme, so one setting themes both views.
use crate::{
    app::App,
    debug,
    editor::{self, Debounce, Tab},
    gtk, launch,
};
use nib_core::{
    LinkTarget, NoteId,
    render::{Anchor, Op, RenderCtx, Styles},
};
use sourceview5::prelude::*;
use std::{
    cell::{Cell, RefCell},
    fmt::Write as _,
    path::Path,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

/// Source, reading, or both side by side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Source,
    Reading,
    Split,
}

const HEADINGS: [(&str, f64); 6] = [
    ("h1", 1.7),
    ("h2", 1.45),
    ("h3", 1.25),
    ("h4", 1.1),
    ("h5", 1.0),
    ("h6", 0.95),
];

thread_local! {
    static TABLE: gtk::TextTagTable = table();
}

/// Every tag the renderer names, created once. Later tags win, so the
/// indent levels are created in order.
fn table() -> gtk::TextTagTable {
    let table = gtk::TextTagTable::new();
    let names = HEADINGS.iter().map(|(n, _)| (*n).to_owned()).chain(
        [
            "em",
            "strong",
            "strike",
            "code",
            "codeblock",
            "code-language",
            "table",
            "quote",
            "callout-title",
            "link",
            "unresolved-link",
            "footnote",
            "footnote-definition",
        ]
        .iter()
        .map(|n| (*n).to_owned()),
    );
    for name in names.chain((0..32).map(|i| format!("indent-{i}"))).chain(
        [
            "embed",
            "embed-title",
            "task",
            "frontmatter",
            "frontmatter-raw",
            "focus-link",
        ]
        .iter()
        .map(|n| (*n).to_owned()),
    ) {
        table.add(&gtk::TextTag::new(Some(&name)));
    }
    structure(&table);
    table
}

/// Sizes, weights, margins and lines: what the scheme's colours must not
/// reset (`GtkSourceStyle::apply` clears the properties a style leaves unset).
fn structure(table: &gtk::TextTagTable) {
    let get = |name: &str| table.lookup(name);
    for (name, scale) in HEADINGS {
        if let Some(t) = get(name) {
            t.set_scale(scale);
            t.set_weight(700);
            t.set_pixels_above_lines(14);
            t.set_pixels_below_lines(4);
        }
    }
    if let Some(t) = get("em") {
        t.set_style(gtk::pango::Style::Italic);
    }
    if let Some(t) = get("strong") {
        t.set_weight(700);
    }
    if let Some(t) = get("strike") {
        t.set_strikethrough(true);
    }
    for name in [
        "code",
        "codeblock",
        "code-language",
        "table",
        "task",
        "frontmatter-raw",
    ] {
        if let Some(t) = get(name) {
            t.set_family(Some("monospace"));
        }
    }
    for name in ["codeblock", "table"] {
        if let Some(t) = get(name) {
            t.set_left_margin(48);
            t.set_wrap_mode(gtk::WrapMode::None);
        }
    }
    if let Some(t) = get("code-language") {
        t.set_scale(0.85);
        t.set_left_margin(48);
    }
    for name in ["quote", "embed"] {
        if let Some(t) = get(name) {
            t.set_left_margin(56);
        }
    }
    for name in ["callout-title", "embed-title", "focus-link"] {
        if let Some(t) = get(name) {
            t.set_weight(700);
        }
    }
    if let Some(t) = get("embed-title") {
        t.set_left_margin(56);
    }
    if let Some(t) = get("link") {
        t.set_underline(gtk::pango::Underline::Single);
    }
    // Pango has no dashed underline: an unresolved link is dim with a low one.
    if let Some(t) = get("unresolved-link") {
        t.set_underline(gtk::pango::Underline::Low);
    }
    if let Some(t) = get("footnote") {
        t.set_rise(6 * gtk::pango::SCALE);
        t.set_scale(0.75);
    }
    for name in ["footnote-definition", "frontmatter", "frontmatter-raw"] {
        if let Some(t) = get(name) {
            t.set_scale(0.9);
        }
    }
    for i in 0..32 {
        if let Some(t) = get(&format!("indent-{i}")) {
            t.set_left_margin(28 + 24 * i);
        }
    }
}

/// Colours from the active scheme: `def:heading`, `def:emphasis`,
/// `def:string`, `def:comment`, `def:link-text` and `text`.
pub fn restyle(scheme: Option<&sourceview5::StyleScheme>) {
    TABLE.with(|table| {
        let apply = |style: &str, tags: &[&str]| {
            let style = scheme.and_then(|s| s.style(style));
            for name in tags {
                if let (Some(style), Some(tag)) = (style.as_ref(), table.lookup(name)) {
                    style.apply(&tag);
                }
            }
        };
        apply("def:heading", &["h1", "h2", "h3", "h4", "h5", "h6"]);
        apply("def:emphasis", &["em"]);
        apply("def:string", &["code", "codeblock", "table"]);
        apply(
            "def:comment",
            &[
                "code-language",
                "quote",
                "unresolved-link",
                "frontmatter",
                "frontmatter-raw",
                "footnote-definition",
                "embed",
            ],
        );
        apply("def:link-text", &["link", "focus-link"]);
        apply("def:strong-emphasis", &["strong"]);
        structure(table);
    });
    let text = scheme.and_then(|s| s.style("text"));
    let (fg, bg) = text
        .map(|t| (t.foreground(), t.background()))
        .unwrap_or_default();
    let mut css = String::new();
    if let Some(bg) = bg {
        let _ = writeln!(
            css,
            "textview.nib-reading, textview.nib-reading text {{ background-color: {bg}; }}"
        );
    }
    if let Some(fg) = fg {
        let _ = writeln!(css, "textview.nib-reading text {{ color: {fg}; }}");
    }
    crate::theme::set_reading_css(&css);
}

/// The reading view's heading colour and the editor's scheme, for `--test-dump`.
pub fn colours(tab: &Tab) -> (String, String) {
    let heading = TABLE.with(|t| {
        t.lookup("h1")
            .filter(TextTagExt::is_foreground_set)
            .map(|t| {
                t.foreground_rgba()
                    .map(|c| c.to_str().into())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    });
    let scheme = tab
        .buffer
        .style_scheme()
        .map(|s| s.id().to_string())
        .unwrap_or_default();
    (heading, scheme)
}

/// A clickable range in the reading buffer (character offsets).
#[derive(Clone)]
enum Hit {
    Link(Arc<LinkTarget>),
    /// A task glyph and its one-based source line.
    Task(u32, bool),
    Frontmatter,
}

pub struct Reading {
    pub view: gtk::TextView,
    pub scroller: gtk::ScrolledWindow,
    hits: RefCell<Vec<(i32, i32, Hit)>>,
    expanded: Cell<bool>,
    focus: Cell<Option<usize>>,
    pending: RefCell<String>,
    pub rerender: RefCell<Option<Rc<Debounce>>>,
    /// Bumped by every render; a queued full pass of an older one is dropped.
    generation: Cell<u64>,
    /// The tab's edit count the view shows, if it is current.
    rendered: Cell<Option<u64>>,
}

/// Link resolution for one render: the current note is the source. The
/// same destination is resolved once per render.
struct Ctx<'a> {
    app: &'a App,
    from: NoteId,
    cache: RefCell<std::collections::HashMap<String, LinkTarget>>,
}

impl RenderCtx for Ctx<'_> {
    fn resolve(&self, raw: &str) -> LinkTarget {
        if let Some(t) = self.cache.borrow().get(raw) {
            return t.clone();
        }
        let t = nib_core::resolve(&self.app.index.borrow(), &self.from, raw);
        self.cache.borrow_mut().insert(raw.to_owned(), t.clone());
        t
    }
    fn resolve_markdown(&self, raw: &str) -> LinkTarget {
        nib_core::links::resolve_markdown(&self.app.index.borrow(), &self.from, raw)
    }
    fn resolve_in(&self, source: &NoteId, raw: &str, kind: nib_core::LinkKind) -> LinkTarget {
        let index = self.app.index.borrow();
        if kind == nib_core::LinkKind::Wiki {
            nib_core::resolve(&index, source, raw)
        } else {
            nib_core::links::resolve_markdown(&index, source, raw)
        }
    }
    fn embed(&self, raw: &str) -> Option<String> {
        let LinkTarget::Note { id, .. } = self.resolve(raw) else {
            return None;
        };
        let path = self.app.vault.root().join(id.filename());
        self.app.vault.read_note(&path).ok().map(|n| n.text)
    }
}

impl Reading {
    fn new(app: &Rc<App>, tab: &Rc<Tab>) -> Rc<Self> {
        let buffer = TABLE.with(|t| gtk::TextBuffer::new(Some(t)));
        buffer.set_enable_undo(false);
        let view = gtk::TextView::with_buffer(&buffer);
        view.add_css_class("nib-reading");
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_top_margin(16);
        view.set_bottom_margin(32);
        let margin = i32::try_from(app.config.borrow().editor.margin).unwrap_or(24);
        view.set_left_margin(margin);
        view.set_right_margin(margin);
        view.set_pixels_below_lines(4);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let reading = Rc::new(Self {
            view,
            scroller,
            hits: RefCell::new(Vec::new()),
            expanded: Cell::new(false),
            focus: Cell::new(None),
            pending: RefCell::new(String::new()),
            rerender: RefCell::new(None),
            generation: Cell::new(0),
            rendered: Cell::new(None),
        });
        reading.connect(app, tab);
        reading
    }

    fn connect(self: &Rc<Self>, app: &Rc<App>, tab: &Rc<Tab>) {
        let click = gtk::GestureClick::new();
        click.set_button(1);
        let (weak, tab_weak) = (app.this.clone(), Rc::downgrade(tab));
        click.connect_released(move |gesture, n, x, y| {
            let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
                return;
            };
            if n != 1 {
                return;
            }
            let Some(reading) = tab.reading.borrow().clone() else {
                return;
            };
            let (bx, by) = reading.view.window_to_buffer_coords(
                gtk::TextWindowType::Widget,
                crate::app::px(x),
                crate::app::px(y),
            );
            if let Some(iter) = reading.view.iter_at_location(bx, by)
                && reading.activate(&app, &tab, iter.offset())
            {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        self.view.add_controller(click);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let (weak, tab_weak) = (app.this.clone(), Rc::downgrade(tab));
        keys.connect_key_pressed(move |_, key, _, state| {
            let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
                return glib::Propagation::Proceed;
            };
            let Some(reading) = tab.reading.borrow().clone() else {
                return glib::Propagation::Proceed;
            };
            if reading.key(&app, &tab, key, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.view.add_controller(keys);
        let (weak, tab_weak) = (app.this.clone(), Rc::downgrade(tab));
        let rerender = Debounce::new(Duration::from_millis(400), move || {
            if let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade())
                && tab.mode.get() == Mode::Split
            {
                render(&app, &tab);
            }
        });
        *self.rerender.borrow_mut() = Some(rerender);
    }

    /// A click (or `Enter`) at a buffer offset: follow a link, toggle a task,
    /// expand the frontmatter. True when something was there.
    fn activate(&self, app: &Rc<App>, tab: &Rc<Tab>, offset: i32) -> bool {
        let hit = self
            .hits
            .borrow()
            .iter()
            .find(|(start, end, _)| (*start..*end).contains(&offset))
            .cloned();
        match hit {
            Some((start, _, Hit::Task(line, done))) => {
                toggle_task(app, tab, self, start, line, done);
                true
            }
            Some((_, _, Hit::Link(target))) => {
                if !app.index_ready.get() && matches!(*target, LinkTarget::Unresolved(_)) {
                    app.status
                        .notice("index not ready — links resolve once indexing finishes");
                } else {
                    launch::follow(app, &target, "");
                }
                true
            }
            Some((_, _, Hit::Frontmatter)) => {
                self.expanded.set(!self.expanded.get());
                self.rendered.set(None);
                render(app, tab);
                true
            }
            None => false,
        }
    }

    fn key(
        &self,
        app: &Rc<App>,
        tab: &Rc<Tab>,
        key: gtk::gdk::Key,
        state: gtk::gdk::ModifierType,
    ) -> bool {
        use gtk::gdk::{Key, ModifierType as M};
        if state.contains(M::ALT_MASK) {
            return false;
        }
        let adj = self.scroller.vadjustment();
        let page = adj.page_size();
        let scroll = |by: f64| adj.set_value((adj.value() + by).clamp(0.0, adj.upper() - page));
        let ctrl = state.contains(M::CONTROL_MASK);
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        match key {
            Key::j | Key::Down if !ctrl => scroll(40.0),
            Key::k | Key::Up if !ctrl => scroll(-40.0),
            Key::d if ctrl => scroll(page / 2.0),
            Key::u if ctrl => scroll(-page / 2.0),
            Key::G => adj.set_value(adj.upper() - page),
            Key::g if pending == "g" => adj.set_value(0.0),
            Key::g => *self.pending.borrow_mut() = "g".into(),
            Key::i => set_mode(app, tab, Mode::Source),
            Key::Tab => self.cycle_link(1),
            Key::ISO_Left_Tab => self.cycle_link(-1),
            Key::Return | Key::KP_Enter => {
                let at = self.focus.get().and_then(|i| {
                    self.hits
                        .borrow()
                        .iter()
                        .filter(|(_, _, h)| matches!(h, Hit::Link(_)))
                        .nth(i)
                        .map(|(s, _, _)| *s)
                });
                if let Some(offset) = at {
                    self.activate(app, tab, offset);
                }
            }
            _ => return false,
        }
        true
    }

    /// `Tab`/`Shift+Tab`: move the focused link, shown in bold.
    fn cycle_link(&self, delta: i64) {
        let links: Vec<(i32, i32)> = self
            .hits
            .borrow()
            .iter()
            .filter(|(_, _, h)| matches!(h, Hit::Link(_)))
            .map(|(s, e, _)| (*s, *e))
            .collect();
        if links.is_empty() {
            return;
        }
        let n = i64::try_from(links.len()).unwrap_or(1);
        let next = self
            .focus
            .get()
            .map_or(if delta > 0 { 0 } else { n - 1 }, |i| {
                (i64::try_from(i).unwrap_or(0) + delta).rem_euclid(n)
            });
        let next = usize::try_from(next).unwrap_or(0);
        self.focus.set(Some(next));
        let buffer = self.view.buffer();
        buffer.remove_tag_by_name("focus-link", &buffer.start_iter(), &buffer.end_iter());
        let (s, e) = links[next];
        let start = buffer.iter_at_offset(s);
        buffer.apply_tag_by_name("focus-link", &start, &buffer.iter_at_offset(e));
        let mut at = start;
        self.view.scroll_to_iter(&mut at, 0.1, false, 0.0, 0.0);
    }

    /// Images shown (decoded) and placeholders still waiting, for `--test-dump`.
    pub fn images(&self) -> (usize, usize) {
        let mut shown = 0;
        let mut waiting = 0;
        let mut child = self.view.first_child();
        while let Some(w) = child {
            if w.has_css_class("nib-image-placeholder") {
                waiting += 1;
            } else if w.first_child().is_some_and(|c| c.is::<gtk::Picture>()) {
                shown += 1;
            }
            child = w.next_sibling();
        }
        (shown, waiting)
    }

    /// Visible text of the reading buffer, for `--test-dump`.
    pub fn text(&self) -> String {
        let buffer = self.view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .into()
    }
}

/// The reading view of a tab, created on first use.
fn ensure(app: &Rc<App>, tab: &Rc<Tab>) -> Rc<Reading> {
    if let Some(r) = tab.reading.borrow().as_ref() {
        return r.clone();
    }
    let reading = Reading::new(app, tab);
    *tab.reading.borrow_mut() = Some(reading.clone());
    reading
}

/// Build the operations for the tab's text and apply them. A long note
/// shows its first 64 KB at once and the whole note one idle later: GTK
/// tags ≈ 0.8 µs per run, so `big.md`'s 30 000 runs alone pass 20 ms.
/// Nothing is done when the text has not changed since the last render.
pub fn render(app: &Rc<App>, tab: &Rc<Tab>) {
    const PART: i32 = 64 * 1024;
    let started = Instant::now();
    let reading = ensure(app, tab);
    let edits = tab.edits.get();
    if reading.rendered.get() == Some(edits) && !tab.image.get() {
        return;
    }
    reading.rendered.set(Some(edits));
    let generation = reading.generation.get() + 1;
    reading.generation.set(generation);
    tab.finish_load();
    if tab.image.get() || tab.buffer.char_count() <= 2 * PART {
        let text = if tab.image.get() {
            String::new()
        } else {
            tab.text()
        };
        render_text(app, tab, &reading, &text);
        debug::trace_ms("reading_render", started);
        return;
    }
    let mut cut = tab.buffer.iter_at_offset(PART);
    if !cut.ends_line() {
        cut.forward_to_line_end();
    }
    let part = tab.buffer.text(&tab.buffer.start_iter(), &cut, true);
    render_text(app, tab, &reading, &part);
    debug::trace_ms("reading_render", started);
    full_pass(app, tab, generation);
}

/// The whole note, after the first part is on screen: ops in one idle, then
/// the tags in slices of ≈ 6 ms, then the swap (keeping the scroll position).
fn full_pass(app: &Rc<App>, tab: &Rc<Tab>, generation: u64) {
    const SLICE: usize = 8_000;
    let (weak, tab_weak) = (app.this.clone(), Rc::downgrade(tab));
    let built: RefCell<Option<Built>> = RefCell::new(None);
    let started = Instant::now();
    glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
            return glib::ControlFlow::Break;
        };
        let Some(reading) = tab.reading.borrow().clone() else {
            return glib::ControlFlow::Break;
        };
        if reading.generation.get() != generation {
            return glib::ControlFlow::Break;
        }
        let mut slot = built.borrow_mut();
        let Some(b) = slot.as_mut() else {
            let text = tab.text();
            *slot = Some(Built::new(&app, &reading, &ops_for(&app, &tab, &text)));
            return glib::ControlFlow::Continue;
        };
        if !b.tag(SLICE) {
            return glib::ControlFlow::Continue;
        }
        if let Some(b) = slot.take() {
            let adj = reading.scroller.vadjustment();
            let value = adj.value();
            b.finish(&reading);
            adj.set_value(value);
            debug::trace_ms("reading_render_full", started);
        }
        glib::ControlFlow::Break
    });
}

fn render_text(app: &Rc<App>, tab: &Rc<Tab>, reading: &Reading, text: &str) {
    let ops = ops_for(app, tab, text);
    apply(app, reading, &ops);
}

fn ops_for<'a>(app: &App, tab: &Tab, text: &'a str) -> Vec<Op<'a>> {
    let started = Instant::now();
    let from = tab
        .path
        .borrow()
        .as_deref()
        .and_then(|p| app.note_id(p))
        .unwrap_or_else(|| NoteId::new("untitled").expect("a constant valid id"));
    let ctx = Ctx {
        app,
        from,
        cache: RefCell::new(std::collections::HashMap::new()),
    };
    let ops = if tab.image.get() {
        let rel = tab
            .path
            .borrow()
            .as_deref()
            .map(|p| app.relative(p))
            .unwrap_or_default();
        vec![Op::Anchor {
            kind: Anchor::Image {
                path: rel,
                width: None,
            },
            lines: 1..2,
        }]
    } else {
        nib_core::render::render(text, &ctx)
    };
    debug::trace_ms("reading_ops", started);
    ops
}

/// Apply operations with one buffer insert: the text is built first with
/// its tag spans, inserted into a fresh buffer no view watches, tagged by
/// walking one iterator forward, and then child anchors go in from the end.
fn apply(app: &Rc<App>, reading: &Reading, ops: &[Op<'_>]) {
    let mut built = Built::new(app, reading, ops);
    built.tag(usize::MAX);
    built.finish(reading);
}

/// A filled buffer not yet shown: text inserted, tags applied in slices.
struct Built {
    buffer: gtk::TextBuffer,
    spans: Vec<(i32, i32, Vec<&'static str>)>,
    next: usize,
    iter: gtk::TextIter,
    at: i32,
    hits: Vec<(i32, i32, Hit)>,
    anchors: Vec<(i32, gtk::Widget)>,
}

impl Built {
    fn new(app: &Rc<App>, reading: &Reading, ops: &[Op<'_>]) -> Self {
        let buffer = TABLE.with(|t| gtk::TextBuffer::new(Some(t)));
        buffer.set_enable_undo(false);
        let mut writer = Writer::default();
        writer.ops(app, reading, ops);
        writer.flush();
        buffer.insert(&mut buffer.start_iter(), &writer.text);
        Self {
            iter: buffer.start_iter(),
            buffer,
            spans: writer.spans,
            next: 0,
            at: 0,
            hits: writer.hits,
            anchors: writer.anchors,
        }
    }

    /// Apply up to `budget` tag spans; true when all are applied. The buffer
    /// is not modified between slices, so the iterator stays valid.
    fn tag(&mut self, budget: usize) -> bool {
        let table = self.buffer.tag_table();
        let end = self.next.saturating_add(budget).min(self.spans.len());
        for (start, stop, names) in &self.spans[self.next..end] {
            self.iter.forward_chars(start - self.at);
            self.at = *start;
            let mut until = self.iter;
            until.forward_chars(stop - start);
            for name in names {
                if let Some(tag) = table.lookup(name) {
                    self.buffer.apply_tag(&tag, &self.iter, &until);
                }
            }
        }
        self.next = end;
        self.next == self.spans.len()
    }

    /// Child anchors (from the end, each shifting what follows), then show.
    fn finish(self, reading: &Reading) {
        let Self {
            buffer,
            mut hits,
            anchors,
            ..
        } = self;
        let mut children = Vec::with_capacity(anchors.len());
        for (offset, widget) in anchors.iter().rev() {
            let mut at = buffer.iter_at_offset(*offset);
            children.push((buffer.create_child_anchor(&mut at), widget.clone()));
        }
        let shift = |x: i32| i32::try_from(anchors.partition_point(|(a, _)| *a <= x)).unwrap_or(0);
        for hit in &mut hits {
            let s = shift(hit.0);
            hit.0 += s;
            hit.1 += s;
        }
        reading.focus.set(None);
        // Freeing a big old buffer takes milliseconds: not before this frame.
        let old = RefCell::new(Some(reading.view.buffer()));
        reading.view.set_buffer(Some(&buffer));
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            drop(old.borrow_mut().take());
            glib::ControlFlow::Break
        });
        for (anchor, widget) in children {
            reading.view.add_child_at_anchor(&widget, &anchor);
        }
        *reading.hits.borrow_mut() = hits;
    }
}

/// Text and tag spans in character offsets (before anchors are inserted).
#[derive(Default)]
struct Writer {
    text: String,
    chars: i32,
    run: String,
    tags: Styles,
    link: Option<Arc<LinkTarget>>,
    /// A tag every run gets (an embed's region).
    extra: Option<&'static str>,
    spans: Vec<(i32, i32, Vec<&'static str>)>,
    hits: Vec<(i32, i32, Hit)>,
    anchors: Vec<(i32, gtk::Widget)>,
}

impl Writer {
    fn same_link(&self, link: Option<&Arc<LinkTarget>>) -> bool {
        match (&self.link, link) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    fn text(&mut self, text: &str, tags: Styles, link: Option<&Arc<LinkTarget>>) {
        if !self.run.is_empty() && (tags != self.tags || !self.same_link(link)) {
            self.flush();
        }
        if self.run.is_empty() {
            self.tags = tags;
            self.link = link.cloned();
        }
        self.run.push_str(text);
    }

    fn push(&mut self, text: &str, names: Vec<&'static str>) -> (i32, i32) {
        let start = self.chars;
        self.chars += i32::try_from(text.chars().count()).unwrap_or(0);
        self.text.push_str(text);
        if !names.is_empty() {
            self.spans.push((start, self.chars, names));
        }
        (start, self.chars)
    }

    fn flush(&mut self) {
        if self.run.is_empty() {
            return;
        }
        let run = std::mem::take(&mut self.run);
        let names: Vec<&'static str> = self.tags.names().chain(self.extra).collect();
        let (start, end) = self.push(&run, names);
        if let Some(link) = self.link.take() {
            self.hits.push((start, end, Hit::Link(link)));
        }
        self.run = run;
        self.run.clear();
    }

    fn insert(&mut self, text: &str, tags: &[&'static str]) -> (i32, i32) {
        self.flush();
        let names = tags.iter().copied().chain(self.extra).collect();
        self.push(text, names)
    }

    fn ops(&mut self, app: &Rc<App>, reading: &Reading, ops: &[Op<'_>]) {
        for op in ops {
            match op {
                Op::Text {
                    text, tags, link, ..
                } => self.text(text, *tags, link.as_ref()),
                Op::Newline { .. } => {
                    // The line break joins the run it ends, unless that is a link.
                    if self.link.is_some() {
                        self.flush();
                    }
                    if self.run.is_empty() {
                        self.tags = Styles::default();
                    }
                    self.run.push('\n');
                }
                Op::Anchor { kind, lines } => self.anchor(app, reading, kind, lines.start),
            }
        }
    }

    fn anchor(&mut self, app: &Rc<App>, reading: &Reading, kind: &Anchor<'_>, line: u32) {
        match kind {
            Anchor::Task(done) => {
                let (s, e) = self.insert(if *done { "☑ " } else { "☐ " }, &["task"]);
                self.hits.push((s, e, Hit::Task(line, *done)));
            }
            Anchor::Rule => {
                self.flush();
                let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
                rule.set_size_request(rule_width(reading), -1);
                rule.set_margin_top(6);
                rule.set_margin_bottom(6);
                self.anchors.push((self.chars, rule.upcast()));
                self.insert("\n", &[]);
            }
            Anchor::Image { path, width } => {
                self.flush();
                let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
                frame.add_css_class("nib-image-placeholder");
                frame.set_size_request(160, 90);
                load_image(app, reading, &frame, path, *width);
                self.anchors.push((self.chars, frame.upcast()));
            }
            Anchor::Embed { target, ops } => {
                let title = app
                    .index
                    .borrow()
                    .notes
                    .iter()
                    .find(|(id, _)| id.as_str() == target.split('#').next().unwrap_or(target))
                    .map_or_else(|| target.clone(), |(_, m)| m.title.clone());
                self.insert(&format!("↳ {title}\n"), &["embed-title"]);
                let outer = self.extra.replace("embed");
                self.ops(app, reading, ops);
                self.flush();
                self.extra = outer;
            }
            Anchor::Frontmatter { raw, properties } => {
                let (s, e) = if reading.expanded.get() {
                    self.insert(raw, &["frontmatter-raw"])
                } else {
                    let label = match properties {
                        1 => "— 1 property —\n".to_owned(),
                        n => format!("— {n} properties —\n"),
                    };
                    self.insert(&label, &["frontmatter"])
                };
                self.hits.push((s, e, Hit::Frontmatter));
            }
        }
    }
}

fn rule_width(reading: &Reading) -> i32 {
    let width = reading.view.width();
    if width > 0 {
        width - 2 * reading.view.left_margin()
    } else {
        480
    }
}

/// Decode a vault-local image at the view's width (or its `|N` hint) off the
/// main thread; the placeholder holds the place until it lands. Remote
/// images never get here: the renderer shows their link text.
fn load_image(app: &Rc<App>, reading: &Reading, frame: &gtk::Box, rel: &str, hint: Option<u32>) {
    let Ok(abs) = app.vault.checked_path(&app.vault.root().join(rel)) else {
        frame.append(&gtk::Label::new(Some(&format!("image not found: {rel}"))));
        frame.set_size_request(-1, -1);
        return;
    };
    let width = hint.map_or_else(|| rule_width(reading), |w| i32::try_from(w).unwrap_or(480));
    let frame = frame.downgrade();
    let rel = rel.to_owned();
    glib::spawn_future_local(async move {
        // Pixbufs are not Send: the worker returns plain pixel bytes.
        let decoded = gio::spawn_blocking(move || {
            let w = gdk_pixbuf::Pixbuf::file_info(&abs).map(|(_, w, _)| w)?;
            let p =
                gdk_pixbuf::Pixbuf::from_file_at_scale(&abs, width.min(w).max(1), -1, true).ok()?;
            Some((
                p.width(),
                p.height(),
                p.rowstride(),
                p.has_alpha(),
                p.read_pixel_bytes(),
            ))
        })
        .await
        .ok()
        .flatten();
        let Some(frame) = frame.upgrade() else { return };
        frame.set_size_request(-1, -1);
        match decoded {
            Some((w, h, stride, alpha, bytes)) => {
                let format = if alpha {
                    gtk::gdk::MemoryFormat::R8g8b8a8
                } else {
                    gtk::gdk::MemoryFormat::R8g8b8
                };
                let texture = gtk::gdk::MemoryTexture::new(
                    w,
                    h,
                    format,
                    &bytes,
                    usize::try_from(stride).unwrap_or(0),
                );
                let picture = gtk::Picture::for_paintable(&texture);
                picture.set_can_shrink(false);
                picture.set_tooltip_text(Some(&rel));
                frame.remove_css_class("nib-image-placeholder");
                frame.append(&picture);
            }
            None => frame.append(&gtk::Label::new(Some(&format!("cannot show {rel}")))),
        }
    });
}

/// Flip a task's source line, save, and swap only its glyph.
fn toggle_task(app: &Rc<App>, tab: &Rc<Tab>, reading: &Reading, at: i32, line: u32, done: bool) {
    if !tab.view.is_editable() {
        return;
    }
    let buffer = &tab.buffer;
    tab.finish_load();
    let Some(mut start) = buffer.iter_at_line(i32::try_from(line).unwrap_or(1) - 1) else {
        return;
    };
    start.set_line_offset(0);
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let text = buffer.text(&start, &end, true);
    let Some(edit) = nib_core::edit::toggle_task(&text, text.len()) else {
        return;
    };
    let chars = |b: usize| i32::try_from(text[..b].chars().count()).unwrap_or(0);
    buffer.begin_user_action();
    let mut from = buffer.iter_at_offset(start.offset() + chars(edit.range.start));
    let mut to = buffer.iter_at_offset(start.offset() + chars(edit.range.end));
    let at_source = from.offset();
    buffer.delete(&mut from, &mut to);
    let mut insert_at = buffer.iter_at_offset(at_source);
    buffer.insert(&mut insert_at, &edit.replacement);
    buffer.end_user_action();
    editor::save(app, tab);
    // The reading view's glyph, in place: no full re-render.
    let rbuf = reading.view.buffer();
    let mut glyph_start = rbuf.iter_at_offset(at);
    let mut glyph_end = rbuf.iter_at_offset(at + 1);
    rbuf.delete(&mut glyph_start, &mut glyph_end);
    let mut glyph_at = rbuf.iter_at_offset(at);
    let task_tag = TABLE.with(|t| t.lookup("task"));
    let glyph = if done { "☐" } else { "☑" };
    match task_tag {
        Some(tag) => rbuf.insert_with_tags(&mut glyph_at, glyph, &[&tag]),
        None => rbuf.insert(&mut glyph_at, glyph),
    }
    for hit in reading.hits.borrow_mut().iter_mut() {
        if hit.0 == at
            && let Hit::Task(_, d) = &mut hit.2
        {
            *d = !done;
        }
    }
}

/// Scroll position as a fraction, for mapping between the two views.
fn ratio(scroller: &gtk::ScrolledWindow) -> f64 {
    let adj = scroller.vadjustment();
    let range = adj.upper() - adj.page_size();
    if range > 0.0 {
        adj.value() / range
    } else {
        0.0
    }
}

fn scroll_to_ratio(scroller: &gtk::ScrolledWindow, ratio: f64) {
    let scroller = scroller.clone();
    // After the next layout, when the adjustment knows the new height.
    glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        let adj = scroller.vadjustment();
        adj.set_value(ratio * (adj.upper() - adj.page_size()).max(0.0));
        glib::ControlFlow::Break
    });
}

/// Put a tab in a mode, rebuilding its content area.
pub fn set_mode(app: &Rc<App>, tab: &Rc<Tab>, mode: Mode) {
    let old = tab.mode.get();
    if old == mode {
        return;
    }
    if tab.image.get() && mode != Mode::Reading {
        return;
    }
    if app.config.borrow().editor.autosave {
        editor::save_if_modified(app, tab);
    }
    let from = match old {
        Mode::Reading => tab
            .reading
            .borrow()
            .as_ref()
            .map_or(0.0, |r| ratio(&r.scroller)),
        _ => ratio(&tab.scroller),
    };
    let content = &tab.content;
    while let Some(child) = content.first_child() {
        if let Some(paned) = child.downcast_ref::<gtk::Paned>() {
            paned.set_start_child(None::<&gtk::Widget>);
            paned.set_end_child(None::<&gtk::Widget>);
        }
        content.remove(&child);
    }
    tab.mode.set(mode);
    match mode {
        Mode::Source => {
            content.append(&tab.scroller);
            scroll_to_ratio(&tab.scroller, from);
            tab.view.grab_focus();
        }
        Mode::Reading => {
            render(app, tab);
            let reading = ensure(app, tab);
            content.append(&reading.scroller);
            scroll_to_ratio(&reading.scroller, from);
            reading.view.grab_focus();
        }
        Mode::Split => {
            render(app, tab);
            let reading = ensure(app, tab);
            let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
            paned.set_start_child(Some(&tab.scroller));
            paned.set_end_child(Some(&reading.scroller));
            paned.set_shrink_start_child(false);
            paned.set_shrink_end_child(false);
            content.append(&paned);
            let width = content.width();
            if width > 0 {
                paned.set_position(width / 2);
            }
            scroll_to_ratio(&reading.scroller, from);
            tab.view.grab_focus();
        }
    }
    app.update_title();
}

/// `Alt+E`, `:read`, `:edit`: source ↔ reading.
pub fn toggle(app: &Rc<App>) {
    let tab = app.tabs.current();
    let next = if tab.mode.get() == Mode::Reading {
        Mode::Source
    } else {
        Mode::Reading
    };
    set_mode(app, &tab, next);
}

/// `Alt+Shift+E`, `:split`.
pub fn toggle_split(app: &Rc<App>) {
    let tab = app.tabs.current();
    let next = if tab.mode.get() == Mode::Split {
        Mode::Source
    } else {
        Mode::Split
    };
    set_mode(app, &tab, next);
}

/// In split mode, keep the reading side at the source side's scroll ratio.
pub fn follow_scroll(tab: &Tab) {
    if tab.mode.get() != Mode::Split {
        return;
    }
    if let Some(r) = tab.reading.borrow().as_ref() {
        let adj = r.scroller.vadjustment();
        adj.set_value(ratio(&tab.scroller) * (adj.upper() - adj.page_size()).max(0.0));
    }
}

/// Image files open in a reading tab showing the picture.
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif"]
            .contains(&e.to_ascii_lowercase().as_str())
    })
}
