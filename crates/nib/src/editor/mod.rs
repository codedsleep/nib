//! One editor tab (P§9): a `GtkSourceView` buffer and view over one note, its
//! remembered text format, and the idle-debounced status work.
pub mod behaviours;
pub mod completion;
pub mod vim;

use crate::{app::App, debug, gtk, reading, theme};
use nib_core::text::TextFormat;
use nib_vault::NoteText;
use sourceview5::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// Runs an action once edits have paused for `delay`, with at most one live
/// timeout (a keystroke only stores a timestamp).
pub struct Debounce {
    delay: Duration,
    last: Cell<Instant>,
    armed: Cell<bool>,
    action: Box<dyn Fn()>,
}

impl Debounce {
    pub fn new(delay: Duration, action: impl Fn() + 'static) -> Rc<Self> {
        Rc::new(Self {
            delay,
            last: Cell::new(Instant::now()),
            armed: Cell::new(false),
            action: Box::new(action),
        })
    }

    pub fn poke(self: &Rc<Self>) {
        self.last.set(Instant::now());
        if !self.armed.replace(true) {
            Self::arm(self, self.delay);
        }
    }

    fn arm(me: &Rc<Self>, after: Duration) {
        let weak = Rc::downgrade(me);
        glib::timeout_add_local_once(after, move || {
            let Some(me) = weak.upgrade() else { return };
            let since = me.last.get().elapsed();
            if since < me.delay {
                Self::arm(&me, me.delay.saturating_sub(since));
            } else {
                me.armed.set(false);
                (me.action)();
            }
        });
    }
}

thread_local! {
    /// The Markdown language, once loaded. Loading it reads every language
    /// file's header (≈35 ms), so the first tab shows plain text in its first
    /// frame and gets highlighting in the post-present idle.
    static MARKDOWN: std::cell::OnceCell<Option<sourceview5::Language>> =
        const { std::cell::OnceCell::new() };
}

/// Load the Markdown language and give it to every open tab.
pub fn load_language(app: &App) {
    let started = Instant::now();
    let lang = MARKDOWN.with(|m| {
        m.get_or_init(|| sourceview5::LanguageManager::default().language("markdown"))
            .clone()
    });
    if let Some(lang) = lang {
        for tab in app.tabs.all() {
            if tab.buffer.language().is_none() {
                tab.buffer.set_language(Some(&lang));
            }
        }
    }
    debug::trace_ms("language", started);
}

pub struct Tab {
    pub id: u64,
    pub path: RefCell<Option<PathBuf>>,
    pub fmt: RefCell<TextFormat>,
    pub read_only: Cell<bool>,
    pub removed: Cell<bool>,
    /// The file changed on disk while the buffer had edits: nothing saves it
    /// automatically until `:w` (overwrite) or `:e!` (reload).
    pub disk_changed: Cell<bool>,
    /// Decoded disk snapshot; automatic saves compare it before writing.
    pub disk_hash: Cell<u64>,
    pub buffer: sourceview5::Buffer,
    pub view: sourceview5::View,
    pub scroller: gtk::ScrolledWindow,
    /// Banner + content; the tab's page in the centre stack.
    pub root: gtk::Box,
    banner: gtk::Label,
    pub words: RefCell<Option<Rc<Debounce>>>,
    /// The vim context while `editor.vim` is on.
    pub vim: RefCell<Option<vim::Vim>>,
    /// nib's own key controller (behaviours, editor chords, completion keys).
    pub keys: gtk::EventControllerKey,
    /// Holds the source scroller, the reading view, or both in a split.
    pub content: gtk::Box,
    pub mode: Cell<reading::Mode>,
    /// The reading view, built on first use.
    pub reading: RefCell<Option<Rc<reading::Reading>>>,
    /// An image file shown in a reading tab (never saved).
    pub image: Cell<bool>,
    /// Counts buffer changes, so the reading view knows when it is stale.
    pub edits: Cell<u64>,
    /// The tab's button in the tab bar and its label.
    pub button: gtk::ToggleButton,
    label: gtk::Label,
    /// When the tab was last current (the 32-tab limit closes the oldest).
    pub used: Cell<u64>,
    /// Notes this tab showed before, and after (`Alt+Left`/`Alt+Right`), with cursor lines.
    pub back: RefCell<Vec<(PathBuf, u32)>>,
    pub forward: RefCell<Vec<(PathBuf, u32)>>,
    /// The tab edits `nib.toml` (`:config`), outside the vault.
    pub config: Cell<bool>,
    /// Saves 1 s after the last edit when `editor.autosave` is on.
    autosave: RefCell<Option<Rc<Debounce>>>,
    /// The rest of a long note still to be inserted after the first frame.
    pending: RefCell<Option<String>>,
    /// Words of the last idle count, shown when the tab becomes current.
    pub word_count: Cell<Option<usize>>,
    app: Weak<App>,
}

impl Tab {
    pub fn new(app: &Rc<App>, id: u64) -> Rc<Self> {
        let config = app.config.borrow();
        let buffer = sourceview5::Buffer::new(None);
        if let Some(Some(lang)) = MARKDOWN.with(|m| m.get().cloned()) {
            buffer.set_language(Some(&lang));
        }
        buffer.set_highlight_matching_brackets(false);
        buffer.set_style_scheme(theme::scheme(&config.editor.style_scheme).as_ref());
        let view = sourceview5::View::with_buffer(&buffer);
        view.add_css_class("nib-editor");
        view.set_top_margin(12);
        view.set_bottom_margin(12);
        view.set_tab_width(4);
        view.set_auto_indent(true);
        view.set_smart_backspace(true);
        view.set_smart_home_end(sourceview5::SmartHomeEndType::Before);
        view.set_highlight_current_line(false);
        view.set_show_right_margin(false);
        apply_settings(&view, &config);
        drop(config);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let banner = gtk::Label::new(None);
        banner.add_css_class("nib-banner");
        banner.set_xalign(0.0);
        banner.set_visible(false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        content.set_vexpand(true);
        content.append(&scroller);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&banner);
        root.append(&content);
        let label = gtk::Label::new(Some("untitled"));
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(24);
        let button = gtk::ToggleButton::new();
        button.set_child(Some(&label));
        button.set_focusable(false);
        let tab = Rc::new_cyclic(|weak| Self {
            keys: behaviours::controller(app, weak),
            id,
            path: RefCell::new(None),
            fmt: RefCell::new(TextFormat::default()),
            read_only: Cell::new(false),
            removed: Cell::new(false),
            disk_changed: Cell::new(false),
            disk_hash: Cell::new(0),
            buffer,
            view,
            scroller,
            root,
            banner,
            words: RefCell::new(None),
            vim: RefCell::new(None),
            content,
            mode: Cell::new(reading::Mode::Source),
            reading: RefCell::new(None),
            image: Cell::new(false),
            edits: Cell::new(0),
            button,
            label,
            used: Cell::new(0),
            back: RefCell::new(Vec::new()),
            forward: RefCell::new(Vec::new()),
            config: Cell::new(false),
            autosave: RefCell::new(None),
            pending: RefCell::new(None),
            word_count: Cell::new(None),
            app: app.this.clone(),
        });
        tab.connect(app);
        behaviours::install(app, &tab);
        vim::sync(app, &tab);
        tab
    }

    fn connect(self: &Rc<Self>, app: &Rc<App>) {
        let weak_tab = Rc::downgrade(self);
        let weak = app.this.clone();
        let words = Debounce::new(Duration::from_millis(300), {
            let weak_tab = weak_tab.clone();
            let weak = weak.clone();
            move || {
                let (Some(tab), Some(app)) = (weak_tab.upgrade(), weak.upgrade()) else {
                    return;
                };
                tab.idle_update(&app);
            }
        });
        *self.words.borrow_mut() = Some(words.clone());
        let autosave = Debounce::new(Duration::from_secs(1), {
            let weak_tab = weak_tab.clone();
            let weak = weak.clone();
            move || {
                let (Some(tab), Some(app)) = (weak_tab.upgrade(), weak.upgrade()) else {
                    return;
                };
                if app.config.borrow().editor.autosave && !tab.config.get() {
                    save_if_modified(&app, &tab);
                }
            }
        });
        *self.autosave.borrow_mut() = Some(autosave.clone());
        let tab_split = weak_tab.clone();
        self.buffer.connect_changed(move |_| {
            words.poke();
            autosave.poke();
            if let Some(tab) = tab_split.upgrade() {
                tab.edits.set(tab.edits.get() + 1);
            }
            // Split view: re-render 400 ms after the last edit (a timestamp now).
            if let Some(tab) = tab_split.upgrade()
                && tab.mode.get() == reading::Mode::Split
                && let Some(r) = tab.reading.borrow().as_ref()
                && let Some(d) = r.rerender.borrow().as_ref()
            {
                d.poke();
            }
        });
        let tab_scroll = weak_tab.clone();
        self.scroller.vadjustment().connect_value_changed(move |_| {
            if let Some(tab) = tab_scroll.upgrade() {
                reading::follow_scroll(&tab);
            }
        });
        let weak2 = weak.clone();
        let tab2 = weak_tab.clone();
        self.buffer.connect_modified_changed(move |_| {
            let (Some(app), Some(tab)) = (weak2.upgrade(), tab2.upgrade()) else {
                return;
            };
            tab.refresh_label(&app);
            if app.tabs.is_current(&tab) {
                app.update_title();
            }
        });
        let tab3 = weak_tab.clone();
        self.buffer.connect_cursor_position_notify(move |_| {
            let (Some(app), Some(tab)) = (weak.upgrade(), tab3.upgrade()) else {
                return;
            };
            if app.tabs.is_current(&tab) {
                tab.show_position(&app);
                app.panel
                    .follow_cursor(u32::try_from(tab.cursor_line()).unwrap_or(1));
                if app.completion.visible() {
                    completion::typed(&app, &tab);
                }
            }
        });
        let weak = app.this.clone();
        self.scroller
            .hadjustment()
            .connect_page_size_notify(move |adj| {
                let (Some(app), Some(tab)) = (weak.upgrade(), weak_tab.upgrade()) else {
                    return;
                };
                tab.fit_width(&app, adj.page_size());
            });
    }

    /// `editor.max_width`: centre the text by widening the side margins, so the
    /// view itself stays the scrollable child (a width-requested box would not be).
    /// Settings and scheme again, after `:config` changed them.
    pub fn reapply(&self, app: &App) {
        let config = app.config.borrow();
        apply_settings(&self.view, &config);
        if !self.config.get() {
            self.buffer
                .set_style_scheme(theme::scheme(&config.editor.style_scheme).as_ref());
        }
        drop(config);
        self.fit_width(app, self.scroller.hadjustment().page_size());
    }

    fn fit_width(&self, app: &App, width: f64) {
        let (margin, max) = {
            let c = app.config.borrow();
            (c.editor.margin, c.editor.max_width)
        };
        let margin = i32::try_from(margin).unwrap_or(24);
        let side = if max == 0 {
            margin
        } else {
            let spare = (width - f64::from(max)) / 2.0;
            margin.max(crate::app::px(spare))
        };
        if self.view.left_margin() != side {
            self.view.set_left_margin(side);
            self.view.set_right_margin(side);
        }
    }

    /// Replace the buffer with a note's text, without undo history.
    pub fn load(&self, app: &Rc<App>, path: &Path, note: NoteText, line: u32) {
        let started = Instant::now();
        *self.path.borrow_mut() = Some(path.to_owned());
        *self.fmt.borrow_mut() = note.fmt;
        self.read_only.set(note.read_only);
        self.removed.set(false);
        self.disk_changed.set(false);
        self.disk_hash.set(nib_core::text::fingerprint(&note.text));
        self.pending.borrow_mut().take();
        self.buffer.begin_irreversible_action();
        match first_chunk(&note.text, line) {
            Some(at) => {
                // A long note at its top: insert a screenful-and-more now, the
                // rest right after the first frame (GtkTextBuffer takes ≈10 ms
                // per 650 KB). Every reader of the text calls `finish_load` first.
                self.buffer.set_text(&note.text[..at]);
                *self.pending.borrow_mut() = Some(note.text[at..].to_owned());
                let weak = self.app.clone();
                let id = self.id;
                glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                    if let Some(tab) = weak.upgrade().and_then(|a| a.tabs.by_id(id)) {
                        tab.finish_load();
                    }
                    glib::ControlFlow::Break
                });
            }
            None => self.buffer.set_text(&note.text),
        }
        self.buffer.end_irreversible_action();
        self.buffer.set_modified(false);
        self.view.set_editable(!note.read_only);
        match note.notice {
            Some(n) => self.show_banner(Some(&n)),
            None => self.show_banner(None),
        }
        let indent = nib_core::edit::detect_indent(&note.text);
        if indent.starts_with(' ') {
            self.view.set_insert_spaces_instead_of_tabs(true);
            self.view
                .set_indent_width(i32::try_from(indent.len()).unwrap_or(4));
        } else {
            self.view.set_insert_spaces_instead_of_tabs(false);
            self.view.set_indent_width(-1);
        }
        self.goto_line(line);
        if let Some(words) = self.words.borrow().as_ref() {
            words.poke();
        }
        debug::trace_paint(&self.view, "open_to_paint", started);
        if app.tabs.is_current_id(self.id) {
            self.show_position(app);
        }
        if let Some(id) = app.note_id(path) {
            app.remember_recent(&id);
        }
    }

    /// Put the cursor at the start of a one-based line and scroll there.
    pub fn goto_line(&self, line: u32) {
        self.goto_position(line, 0);
    }

    /// Restore a one-based line and zero-based character column, clamped to its end.
    pub fn goto_position(&self, line: u32, column: u32) {
        let line = i32::try_from(line.saturating_sub(1)).unwrap_or(0);
        let mut iter = self
            .buffer
            .iter_at_line(line)
            .unwrap_or_else(|| self.buffer.start_iter());
        let mut end = iter;
        end.forward_to_line_end();
        iter.set_line_offset(
            i32::try_from(column)
                .unwrap_or(i32::MAX)
                .min(end.line_offset()),
        );
        self.buffer.place_cursor(&iter);
        let mark = self.buffer.get_insert();
        self.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.25);
    }

    pub fn show_banner(&self, text: Option<&str>) {
        match text {
            Some(t) => {
                self.banner.set_text(t);
                self.banner.set_visible(true);
            }
            None => self.banner.set_visible(false),
        }
    }

    pub fn show_position(&self, app: &App) {
        let iter = self.buffer.iter_at_mark(&self.buffer.get_insert());
        app.status
            .set_position(iter.line() + 1, iter.line_offset() + 1);
    }

    /// Work done 300 ms after the last edit: word count and outline.
    pub fn idle_update(&self, app: &Rc<App>) {
        let text = self.text();
        let words = nib_core::edit::word_count(&text);
        self.word_count.set(Some(words));
        if app.tabs.is_current_id(self.id) {
            app.status.set_words(Some(words));
            let line = u32::try_from(self.cursor_line()).unwrap_or(1);
            app.panel.set_outline(nib_core::edit::outline(&text), line);
        }
    }

    /// The tab bar label: the note's title, `●` when modified.
    pub fn refresh_label(&self, app: &App) {
        let title = self.title(app);
        let text = if self.buffer.is_modified() {
            format!("{title} ●")
        } else {
            title
        };
        self.label.set_text(&text);
        self.button.set_tooltip_text(Some(&self.display_path(app)));
    }

    /// One-based cursor line.
    pub fn cursor_line(&self) -> i32 {
        self.buffer.iter_at_mark(&self.buffer.get_insert()).line() + 1
    }

    /// Insert the remainder of a two-step load now, if any is left.
    pub fn finish_load(&self) {
        let Some(rest) = self.pending.borrow_mut().take() else {
            return;
        };
        let modified = self.buffer.is_modified();
        self.buffer.begin_irreversible_action();
        self.buffer.insert(&mut self.buffer.end_iter(), &rest);
        self.buffer.end_irreversible_action();
        if !modified {
            self.buffer.set_modified(false);
        }
    }

    pub fn text(&self) -> String {
        self.finish_load();
        let (start, end) = self.buffer.bounds();
        self.buffer.text(&start, &end, true).into()
    }

    pub fn title(&self, app: &App) -> String {
        let path = self.path.borrow();
        let Some(path) = path.as_ref() else {
            return "untitled".into();
        };
        if let Some(id) = app.note_id(path) {
            if let Some(meta) = app.index.borrow().notes.get(&id) {
                return meta.title.clone();
            }
            return id.stem().to_owned();
        }
        path.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    pub fn display_path(&self, app: &App) -> String {
        self.path
            .borrow()
            .as_ref()
            .map_or_else(|| "untitled".into(), |p| app.relative(p))
    }
}

/// Where to split a long note's first insertion: past 64 KB of text, at a
/// line end, only when the cursor goes to the top.
fn first_chunk(text: &str, line: u32) -> Option<usize> {
    const CHUNK: usize = 64 * 1024;
    if line > 1 || text.len() <= 2 * CHUNK {
        return None;
    }
    let mut at = CHUNK;
    while !text.is_char_boundary(at) {
        at += 1;
    }
    text[at..].find('\n').map(|i| at + i + 1)
}

/// View settings that follow `[editor]` and can change live.
pub fn apply_settings(view: &sourceview5::View, config: &nib_core::settings::Config) {
    view.set_wrap_mode(if config.editor.wrap {
        gtk::WrapMode::WordChar
    } else {
        gtk::WrapMode::None
    });
    view.set_show_line_numbers(config.editor.line_numbers);
    let margin = i32::try_from(config.editor.margin).unwrap_or(24);
    if config.editor.max_width == 0 {
        view.set_left_margin(margin);
        view.set_right_margin(margin);
    }
}

/// Save one tab now (`Ctrl+S`, `:w`).
pub fn save(app: &Rc<App>, tab: &Tab) -> bool {
    save_checked(app, tab, true)
}

fn save_checked(app: &Rc<App>, tab: &Tab, explicit: bool) -> bool {
    if tab.config.get() {
        return crate::config::save(app, tab);
    }
    if tab.read_only.get() {
        app.status.error(&format!(
            "{} is read-only (not UTF-8); nib never saves it",
            tab.display_path(app)
        ));
        return false;
    }
    let Some(path) = tab.path.borrow().clone() else {
        app.status.error("untitled: name it with :w NAME");
        return false;
    };
    let text = tab.text();
    let fmt = tab.fmt.borrow().clone();
    let result = if explicit {
        app.vault.save(&path, &text, &fmt)
    } else {
        app.vault
            .save_unchanged(&path, &text, &fmt, tab.disk_hash.get())
    };
    match result {
        Ok(_) => {
            tab.buffer.set_modified(false);
            tab.disk_changed.set(false);
            tab.disk_hash.set(nib_core::text::fingerprint(&text));
            if tab.removed.replace(false) {
                app.status
                    .notice(&format!("recreated {}", app.relative(&path)));
            }
            true
        }
        Err(e) => {
            app.status
                .error(&format!("cannot save {}: {e}", app.relative(&path)));
            false
        }
    }
}

/// The automatic save (autosave, tab switch, focus-out, before a rename plan
/// or a search). It never writes over a change made on disk, nor recreates a
/// removed note: those wait for an explicit `:w` (P§16).
pub fn save_if_modified(app: &App, tab: &Tab) -> bool {
    if !tab.buffer.is_modified() {
        return true;
    }
    if app.config.borrow().editor.autosave
        && !tab.config.get()
        && tab.path.borrow().is_some()
        && !tab.read_only.get()
        && !tab.disk_changed.get()
        && !tab.removed.get()
        && let Some(app) = app.this.upgrade()
    {
        return save_checked(&app, tab, false);
    }
    false
}

/// Give an untitled tab its note: created in the new-note folder, never over
/// an existing file.
pub fn save_as(app: &Rc<App>, tab: &Tab, name: &str) {
    let Some(id) = crate::launch::new_note_id(app, name) else {
        app.status
            .error(&format!("cannot make a note called {name}"));
        return;
    };
    let text = tab.text();
    match app.vault.create_note(&id, &text) {
        Ok(path) => {
            *tab.path.borrow_mut() = Some(path);
            *tab.fmt.borrow_mut() = TextFormat::default();
            tab.disk_hash.set(nib_core::text::fingerprint(&text));
            tab.buffer.set_modified(false);
            app.update_title();
            app.status.notice(&format!("created {}", id.filename()));
        }
        Err(e) => app
            .status
            .error(&format!("cannot create {}: {e}", id.filename())),
    }
}

/// Alt+2: focus the current tab's view (the reading view in reading mode).
pub fn focus(app: &Rc<App>) {
    app.tabs.focus_current();
}
