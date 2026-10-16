//! The right panel (P§12): backlinks grouped by source note, with the
//! linking line and the link in bold, the note's unresolved links, and a
//! live outline of the buffer that follows the cursor's section.
use crate::{app::App, gtk};
use gtk::prelude::*;
use nib_core::{Heading, LinkTarget, NoteId};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Backlinks,
    Outline,
    /// Whichever page is showing.
    Current,
}

/// A backlinks-page row.
enum Row {
    /// A source note's title.
    Source { title: String, note: NoteId },
    /// A linking line, Pango markup with the link in bold.
    Line {
        markup: String,
        note: NoteId,
        line: u32,
    },
    /// A section heading or an explanation.
    Label(String),
    /// An unresolved link in the current note.
    Unresolved(String),
}

/// An outline row.
struct Outline {
    text: String,
    level: u8,
    line: u32,
}

/// The note, its backlinks and its unresolved links, as last shown.
type Shown = (Option<NoteId>, Vec<(NoteId, u32)>, Vec<String>);

pub struct Panel {
    pub root: gtk::Box,
    backlinks_button: gtk::ToggleButton,
    outline_button: gtk::ToggleButton,
    stack: gtk::Stack,
    links: gio::ListStore,
    links_list: gtk::ListView,
    outline: gio::ListStore,
    outline_selection: gtk::SingleSelection,
    outline_list: gtk::ListView,
    /// The note and backlinks shown, to skip identical refreshes.
    shown: RefCell<Option<Shown>>,
    headings: RefCell<Vec<Heading>>,
    generation: Cell<u64>,
    queued: Cell<bool>,
}

fn boxed<T: 'static>(obj: &glib::Object) -> Option<std::cell::Ref<'_, T>> {
    obj.downcast_ref::<glib::BoxedAnyObject>()
        .and_then(|b| b.try_borrow::<T>().ok())
}

fn label_factory(bind: fn(&gtk::Label, &glib::Object)) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(label), Some(obj)) = (item.child().and_downcast::<gtk::Label>(), item.item()) {
            bind(&label, &obj);
        }
    });
    factory
}

impl Panel {
    pub fn new(app: &std::rc::Weak<App>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("nib-panel");
        root.set_width_request(160);
        let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        switcher.add_css_class("nib-tabbar");
        let backlinks_button = gtk::ToggleButton::with_label("Backlinks");
        let outline_button = gtk::ToggleButton::with_label("Outline");
        outline_button.set_group(Some(&backlinks_button));
        backlinks_button.set_active(true);
        for b in [&backlinks_button, &outline_button] {
            b.set_focusable(false);
            switcher.append(b);
        }
        let links = gio::ListStore::new::<glib::BoxedAnyObject>();
        let links_list = gtk::ListView::new(
            Some(gtk::SingleSelection::new(Some(links.clone()))),
            Some(label_factory(|label, obj| {
                let Some(row) = boxed::<Row>(obj) else { return };
                match &*row {
                    Row::Source { title, .. } => {
                        label.set_text(title);
                        label.set_css_classes(&["heading"]);
                    }
                    Row::Line { markup, .. } => {
                        label.set_markup(markup);
                        label.set_css_classes(&[]);
                    }
                    Row::Label(text) => {
                        label.set_text(text);
                        label.set_css_classes(&["dim"]);
                    }
                    Row::Unresolved(name) => {
                        label.set_text(name);
                        label.set_css_classes(&["dim"]);
                    }
                }
            })),
        );
        crate::app::click_activates(&links_list);
        let outline = gio::ListStore::new::<glib::BoxedAnyObject>();
        let outline_selection = gtk::SingleSelection::new(Some(outline.clone()));
        outline_selection.set_autoselect(false);
        outline_selection.set_can_unselect(true);
        let outline_list = gtk::ListView::new(
            Some(outline_selection.clone()),
            Some(label_factory(|label, obj| {
                if let Some(row) = boxed::<Outline>(obj) {
                    label.set_text(&row.text);
                    label.set_margin_start(i32::from(row.level.saturating_sub(1)) * 12);
                }
            })),
        );
        crate::app::click_activates(&outline_list);
        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        let scrolled = |child: &gtk::ListView| {
            gtk::ScrolledWindow::builder()
                .child(child)
                .vexpand(true)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .build()
        };
        stack.add_named(&scrolled(&links_list), Some("backlinks"));
        stack.add_named(&scrolled(&outline_list), Some("outline"));
        root.append(&switcher);
        root.append(&stack);
        let panel = Self {
            root,
            backlinks_button,
            outline_button,
            stack,
            links,
            links_list,
            outline,
            outline_selection,
            outline_list,
            shown: RefCell::new(None),
            headings: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            queued: Cell::new(false),
        };
        panel.connect(app);
        panel
    }

    fn connect(&self, app: &std::rc::Weak<App>) {
        let stack = self.stack.clone();
        self.backlinks_button.connect_toggled(move |b| {
            if b.is_active() {
                stack.set_visible_child_name("backlinks");
            }
        });
        let stack = self.stack.clone();
        self.outline_button.connect_toggled(move |b| {
            if b.is_active() {
                stack.set_visible_child_name("outline");
            }
        });
        let weak = app.clone();
        self.links_list.connect_activate(move |list, pos| {
            let Some(app) = weak.upgrade() else { return };
            let Some(obj) = list.model().and_then(|m| m.item(pos)) else {
                return;
            };
            let target = match boxed::<Row>(&obj).as_deref() {
                Some(Row::Source { note, .. }) => Some((note.clone(), 1)),
                Some(Row::Line { note, line, .. }) => Some((note.clone(), *line)),
                Some(Row::Unresolved(name)) => {
                    crate::launch::create_unresolved(&app, name);
                    None
                }
                _ => None,
            };
            if let Some((note, line)) = target {
                let path = app.vault.root().join(note.filename());
                app.open_note(&path, false, Some(line));
            }
        });
        let weak = app.clone();
        self.outline_list.connect_activate(move |list, pos| {
            let Some(app) = weak.upgrade() else { return };
            let line = list
                .model()
                .and_then(|m| m.item(pos))
                .and_then(|o| boxed::<Outline>(&o).map(|r| r.line));
            if let Some(line) = line {
                let tab = app.tabs.current();
                tab.goto_line(line);
                tab.view.grab_focus();
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let backlinks = self.backlinks_button.clone();
        let outline = self.outline_button.clone();
        let weak = app.clone();
        keys.connect_key_pressed(move |_, key, _, state| {
            if key == gtk::gdk::Key::Tab && state.is_empty() {
                if backlinks.is_active() {
                    outline.set_active(true);
                } else {
                    backlinks.set_active(true);
                }
                if let Some(app) = weak.upgrade() {
                    app.panel.focus_list();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.root.add_controller(keys);
    }

    pub fn page(&self) -> &'static str {
        if self.backlinks_button.is_active() {
            "backlinks"
        } else {
            "outline"
        }
    }

    fn focus_list(&self) {
        let list = if self.backlinks_button.is_active() {
            &self.links_list
        } else {
            &self.outline_list
        };
        list.grab_focus();
        if list.model().is_some_and(|m| m.n_items() > 0) {
            list.scroll_to(0, gtk::ListScrollFlags::FOCUS, None);
        }
    }

    /// Refresh after the index or the current note changed, once per idle.
    pub fn queue(&self, app: &Rc<App>) {
        if self.queued.replace(true) {
            return;
        }
        let weak = app.this.clone();
        glib::idle_add_local_once(move || {
            if let Some(app) = weak.upgrade() {
                app.panel.queued.set(false);
                app.panel.refresh(&app);
            }
        });
    }

    fn refresh(&self, app: &Rc<App>) {
        let tab = app.tabs.current();
        let note = tab.path.borrow().as_deref().and_then(|p| app.note_id(p));
        let (sources, unresolved) = match (&note, app.index_ready.get()) {
            (Some(id), true) => {
                let index = app.index.borrow();
                let mut sources = index.backlinks(id).to_vec();
                sources.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                let mut unresolved: Vec<String> = index
                    .notes
                    .get(id)
                    .map(|m| {
                        m.links
                            .iter()
                            .filter_map(|l| match &l.target {
                                LinkTarget::Unresolved(name) => Some(name.clone()),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                unresolved.sort();
                unresolved.dedup();
                (sources, unresolved)
            }
            _ => (vec![], vec![]),
        };
        let key = (note.clone(), sources.clone(), unresolved.clone());
        if self.shown.borrow().as_ref() == Some(&key) {
            return;
        }
        *self.shown.borrow_mut() = Some(key);
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let Some(note) = note else {
            self.set_rows(vec![Row::Label("no note".into())]);
            return;
        };
        if !app.index_ready.get() {
            self.set_rows(vec![Row::Label("index not ready".into())]);
            return;
        }
        // Read the linking lines off the main thread.
        let vault = app.vault.clone();
        let root = app.vault.root().to_owned();
        let wanted: Vec<(NoteId, u32)> = sources;
        let weak = app.this.clone();
        glib::spawn_future_local(async move {
            let lines = gio::spawn_blocking(move || read_lines(&vault, &root, &wanted))
                .await
                .unwrap_or_default();
            let Some(app) = weak.upgrade() else { return };
            if app.panel.generation.get() != generation {
                return;
            }
            let rows = Self::rows(&app, &note, &lines, &unresolved);
            app.panel.set_rows(rows);
        });
    }

    fn rows(
        app: &App,
        note: &NoteId,
        lines: &[(NoteId, u32, String, usize)],
        unresolved: &[String],
    ) -> Vec<Row> {
        let index = app.index.borrow();
        let mut rows = Vec::new();
        let sources = lines
            .iter()
            .map(|(s, ..)| s)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        rows.push(Row::Label(match sources {
            0 => "no backlinks".into(),
            1 => "1 linking note".into(),
            n => format!("{n} linking notes"),
        }));
        let mut last: Option<&NoteId> = None;
        for (source, line, text, start) in lines {
            if last != Some(source) {
                let title = index
                    .notes
                    .get(source)
                    .map_or_else(|| source.to_string(), |m| m.title.clone());
                rows.push(Row::Source {
                    title,
                    note: source.clone(),
                });
                last = Some(source);
            }
            let spans: Vec<std::ops::Range<usize>> = index
                .notes
                .get(source)
                .map(|m| {
                    m.links
                        .iter()
                        .filter(|l| {
                            l.line == *line
                                && matches!(&l.target, LinkTarget::Note { id, .. } if id == note)
                        })
                        .map(|l| l.span.clone())
                        .collect()
                })
                .unwrap_or_default();
            rows.push(Row::Line {
                markup: bold_spans(text, *start, &spans),
                note: source.clone(),
                line: *line,
            });
        }
        if !unresolved.is_empty() {
            rows.push(Row::Label("unresolved in this note".into()));
            rows.extend(unresolved.iter().cloned().map(Row::Unresolved));
        }
        rows
    }

    fn set_rows(&self, rows: Vec<Row>) {
        let objects: Vec<glib::Object> = rows
            .into_iter()
            .map(|r| glib::BoxedAnyObject::new(r).upcast())
            .collect();
        self.links.splice(0, self.links.n_items(), &objects);
    }

    /// Rebuild the outline from the buffer's headings (300 ms idle).
    pub fn set_outline(&self, headings: Vec<Heading>, cursor_line: u32) {
        let unchanged = {
            let old = self.headings.borrow();
            old.len() == headings.len()
                && old
                    .iter()
                    .zip(&headings)
                    .all(|(a, b)| a.text == b.text && a.level == b.level && a.line == b.line)
        };
        if !unchanged {
            let objects: Vec<glib::Object> = headings
                .iter()
                .map(|h| {
                    glib::BoxedAnyObject::new(Outline {
                        text: h.text.clone(),
                        level: h.level,
                        line: h.line,
                    })
                    .upcast()
                })
                .collect();
            self.outline.splice(0, self.outline.n_items(), &objects);
            *self.headings.borrow_mut() = headings;
        }
        self.follow_cursor(cursor_line);
    }

    /// Mark the section the cursor is in.
    pub fn follow_cursor(&self, line: u32) {
        let at = self.headings.borrow().iter().rposition(|h| h.line <= line);
        let pos = at.map_or(gtk::INVALID_LIST_POSITION, |i| {
            u32::try_from(i).unwrap_or(0)
        });
        if self.outline_selection.selected() != pos {
            self.outline_selection.set_selected(pos);
        }
    }

    /// Visible rows of the current page, for `--test-dump`.
    pub fn texts(&self) -> Vec<String> {
        let store = if self.backlinks_button.is_active() {
            &self.links
        } else {
            &self.outline
        };
        (0..store.n_items())
            .filter_map(|i| store.item(i))
            .filter_map(|o| {
                if let Some(r) = boxed::<Row>(&o) {
                    return Some(match &*r {
                        Row::Source { title, .. } => format!("# {title}"),
                        Row::Line { markup, .. } => markup.clone(),
                        Row::Label(t) => format!("({t})"),
                        Row::Unresolved(n) => format!("? {n}"),
                    });
                }
                boxed::<Outline>(&o)
                    .map(|r| format!("{}{}", "  ".repeat(usize::from(r.level)), r.text))
            })
            .collect()
    }

    pub fn current_section(&self) -> Option<String> {
        let pos = self.outline_selection.selected();
        self.headings
            .borrow()
            .get(usize::try_from(pos).ok()?)
            .map(|h| h.text.clone())
    }
}

/// For each backlink (source, line): the line's text and its byte offset.
fn read_lines(
    vault: &nib_vault::VaultHandle,
    root: &std::path::Path,
    wanted: &[(NoteId, u32)],
) -> Vec<(NoteId, u32, String, usize)> {
    let mut out = Vec::with_capacity(wanted.len());
    let mut cache: Option<(NoteId, String)> = None;
    for (source, line) in wanted {
        if cache.as_ref().is_none_or(|(id, _)| id != source) {
            let path: PathBuf = root.join(source.filename());
            let text = vault.read_note(&path).map(|n| n.text).unwrap_or_default();
            cache = Some((source.clone(), text));
        }
        let Some((_, text)) = &cache else { continue };
        let mut start = 0;
        let mut found = None;
        for (i, l) in text.split('\n').enumerate() {
            if i + 1 == usize::try_from(*line).unwrap_or(usize::MAX) {
                found = Some((l.trim_end_matches('\r').to_owned(), start));
                break;
            }
            start += l.len() + 1;
        }
        if let Some((l, s)) = found {
            out.push((source.clone(), *line, l, s));
        }
    }
    out
}

/// Escape a line for Pango, bolding the given absolute byte spans, trimmed.
fn bold_spans(line: &str, line_start: usize, spans: &[std::ops::Range<usize>]) -> String {
    let lead = line.len() - line.trim_start().len();
    let text = line.trim();
    let base = line_start + lead;
    let mut out = String::with_capacity(text.len() + 16);
    let mut at = 0;
    let mut spans: Vec<_> = spans
        .iter()
        .filter_map(|s| {
            let a = s.start.checked_sub(base)?;
            let b = s.end.saturating_sub(base).min(text.len());
            (a < b && text.is_char_boundary(a) && text.is_char_boundary(b)).then_some(a..b)
        })
        .collect();
    spans.sort_by_key(|s| s.start);
    for s in spans {
        if s.start < at {
            continue;
        }
        out.push_str(&glib::markup_escape_text(&text[at..s.start]));
        out.push_str("<b>");
        out.push_str(&glib::markup_escape_text(&text[s.clone()]));
        out.push_str("</b>");
        at = s.end;
    }
    out.push_str(&glib::markup_escape_text(&text[at..]));
    out
}

/// F8.
pub fn toggle(app: &Rc<App>) {
    let visible = !app.panel.root.is_visible();
    if !visible && app.panel.root.focus_child().is_some() {
        crate::editor::focus(app);
    }
    app.panel.root.set_visible(visible);
    app.session.borrow_mut().right_visible = visible;
    if visible {
        app.panel.queue(app);
    }
}

/// `Alt+B`, `Alt+3`, `:backlinks`, `:outline`: show the panel and focus a page.
pub fn focus(app: &Rc<App>, page: Page) {
    app.panel.root.set_visible(true);
    app.session.borrow_mut().right_visible = true;
    match page {
        Page::Backlinks => app.panel.backlinks_button.set_active(true),
        Page::Outline => app.panel.outline_button.set_active(true),
        Page::Current => {}
    }
    app.panel.queue(app);
    app.panel.focus_list();
}

#[cfg(test)]
mod tests {
    #[test]
    fn bold() {
        // "  see [[hub]] & more" starting at byte 10; the link spans 16..23.
        assert_eq!(
            super::bold_spans("  see [[hub]] & more", 10, std::slice::from_ref(&(16..23))),
            "see <b>[[hub]]</b> &amp; more"
        );
    }
}
