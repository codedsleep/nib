//! Application lifetime: one window per vault, its start-up path and the
//! vault event stream.
use crate::{debug, editor, fileops, files, finder, gtk, panel, registry, status, tabs, theme};
use gtk::prelude::*;
use nib_core::{Index, NoteId, settings::Config};
use nib_vault::{
    Guard, IndexEvent, VaultEvent, VaultHandle,
    state::{self, Paths, Session},
};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    time::Instant,
};

/// Per-process state shared by every command line the primary instance gets.
pub struct Launcher {
    t0: Instant,
    paths: Paths,
    app: RefCell<Option<Rc<App>>>,
    exit: Cell<Option<glib::ExitCode>>,
    themed: Cell<bool>,
}

impl Launcher {
    pub fn new(t0: Instant, paths: Paths, _profile: Option<String>) -> Rc<Self> {
        Rc::new(Self {
            t0,
            paths,
            app: RefCell::new(None),
            exit: Cell::new(None),
            themed: Cell::new(false),
        })
    }

    /// A failing status of the first command line, which `run()` loses once
    /// the main loop has turned.
    pub fn exit_status(&self) -> Option<glib::ExitCode> {
        self.exit.get()
    }

    /// Persist the session when the main loop ends.
    pub fn shutdown(&self) {
        if let Some(app) = self.app.borrow_mut().take() {
            app.shutdown();
        }
    }

    fn fail(&self, cmd: &gio::ApplicationCommandLine, message: &str) -> glib::ExitCode {
        cmd.printerr_literal(&format!("nib: {message}\n"));
        let code = glib::ExitCode::from(1);
        if self.app.borrow().is_none() {
            self.exit.set(Some(code));
        }
        code
    }

    pub fn command_line(
        self: &Rc<Self>,
        gapp: &gtk::Application,
        cmd: &gio::ApplicationCommandLine,
    ) -> glib::ExitCode {
        let args = cmd
            .arguments()
            .into_iter()
            .skip(1)
            .map(|a| a.to_string_lossy().into_owned());
        let inv = match nib_core::cli::parse(args) {
            Ok(i) => i,
            Err(e) => {
                cmd.printerr_literal(&format!("nib: {e}\n"));
                return glib::ExitCode::from(2);
            }
        };
        let cwd = cmd.cwd().unwrap_or_else(|| PathBuf::from("/"));
        let absolute = |p: &Path| {
            if p.is_absolute() {
                p.to_owned()
            } else {
                cwd.join(p)
            }
        };
        let running = self.app.borrow().clone();
        if let Some(app) = running {
            if inv.test_dump {
                cmd.print_literal(&debug::dump(&app));
                return glib::ExitCode::SUCCESS;
            }
            if let Some(keys) = &inv.test_keys {
                return debug::inject(keys);
            }
            app.invocation(&inv, inv.path.as_deref().map(absolute));
            app.window.present();
            return glib::ExitCode::SUCCESS;
        }
        if inv.test_dump || inv.test_keys.is_some() {
            return self.fail(cmd, "no window is open");
        }
        let guard = Guard::from_env();
        let (config, warnings) = match state::load_config(&self.paths, &guard) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("config: {e}");
                (
                    Config::default(),
                    vec![nib_core::settings::Warning(e.to_string())],
                )
            }
        };
        let path = inv.path.as_deref().map(absolute);
        let explicit = inv.vault.as_deref().map(absolute);
        let (root, note) = match &path {
            Some(p) if p.is_dir() => (state::find_vault_root(explicit.as_deref(), p), None),
            Some(p) => (
                state::find_vault_root(explicit.as_deref(), p),
                Some(p.clone()),
            ),
            None => match explicit {
                Some(v) => (v.canonicalize().map_err(Into::into), None),
                None if !config.vault.last.is_empty() && Path::new(&config.vault.last).is_dir() => {
                    (Ok(PathBuf::from(&config.vault.last)), None)
                }
                None => {
                    self.choose_vault(gapp, config, warnings, inv);
                    return glib::ExitCode::SUCCESS;
                }
            },
        };
        let root = match root {
            Ok(r) => r,
            Err(e) => return self.fail(cmd, &format!("cannot open the vault: {e}")),
        };
        if let Err(e) = self.open(gapp, &root, config, warnings, note, &inv) {
            return self.fail(cmd, &e.to_string());
        }
        glib::ExitCode::SUCCESS
    }

    pub fn open_files(self: &Rc<Self>, gapp: &gtk::Application, files: &[gio::File]) {
        let app = self.app.borrow().clone();
        if let Some(app) = app {
            for file in files {
                if let Some(path) = file.path() {
                    app.open_path(&path, false);
                }
            }
            app.window.present();
        } else if let Some(path) = files.first().and_then(gio::File::path) {
            let guard = Guard::from_env();
            let (config, warnings) = state::load_config(&self.paths, &guard).unwrap_or_default();
            if let Ok(root) = state::find_vault_root(None, &path) {
                let inv = nib_core::cli::Invocation::default();
                let note = (!path.is_dir()).then_some(path);
                if let Err(e) = self.open(gapp, &root, config, warnings, note, &inv) {
                    log::warn!("{e}");
                }
            }
        }
    }

    /// First run: no vault argument and no remembered vault.
    fn choose_vault(
        self: &Rc<Self>,
        gapp: &gtk::Application,
        config: Config,
        warnings: Vec<nib_core::settings::Warning>,
        inv: nib_core::cli::Invocation,
    ) {
        self.theme_once();
        let hold = gapp.hold();
        let dialog = gtk::FileDialog::builder()
            .title("Choose a vault folder")
            .modal(true)
            .build();
        let me = self.clone();
        let gapp = gapp.clone();
        dialog.select_folder(
            None::<&gtk::Window>,
            None::<&gio::Cancellable>,
            move |res| {
                let _hold = hold;
                match res.ok().and_then(|f| f.path()) {
                    Some(root) => {
                        if let Err(e) = me.open(&gapp, &root, config, warnings, None, &inv) {
                            eprintln!("nib: {e}");
                            me.exit.set(Some(glib::ExitCode::from(1)));
                        }
                    }
                    None => me.exit.set(Some(glib::ExitCode::from(1))),
                }
            },
        );
    }

    fn theme_once(&self) {
        if !self.themed.replace(true) {
            theme::install();
        }
    }

    fn open(
        self: &Rc<Self>,
        gapp: &gtk::Application,
        root: &Path,
        config: Config,
        warnings: Vec<nib_core::settings::Warning>,
        note: Option<PathBuf>,
        inv: &nib_core::cli::Invocation,
    ) -> anyhow::Result<()> {
        self.theme_once();
        let app = App::new(gapp, self.t0, self.paths.clone(), root, config, note)?;
        for w in warnings {
            app.status.error(&format!("nib.toml: {}", w.0));
        }
        *self.app.borrow_mut() = Some(app.clone());
        app.invocation(inv, None);
        Ok(())
    }
}

/// The switcher's candidates with the index generation and recent list they were built from.
type SwitcherCache = (u64, Vec<NoteId>, Vec<nib_core::fuzzy::Candidate>);

/// One open vault and its window. Components hold `Weak<App>` in their
/// signal handlers and upgrade on use.
pub struct App {
    pub window: gtk::ApplicationWindow,
    pub vault: VaultHandle,
    pub paths: Paths,
    pub guard: Guard,
    pub key: String,
    pub config: RefCell<Config>,
    pub index: RefCell<Index>,
    pub index_ready: Cell<bool>,
    pub status: status::Status,
    pub files: files::Files,
    pub tabs: tabs::Tabs,
    pub outer: gtk::Paned,
    pub inner: gtk::Paned,
    pub panel: panel::Panel,
    pub finder: finder::Finder,
    /// Running file operations and what to do when each finishes.
    pub ops: RefCell<std::collections::HashMap<nib_vault::OpId, fileops::Pending>>,
    /// Bumped by every index change; the switcher's candidates follow it.
    pub index_generation: Cell<u64>,
    switcher: RefCell<Option<SwitcherCache>>,
    pub completion: editor::completion::Completion,
    /// The question of an open dialog, for `--test-dump`.
    pub dialog: RefCell<Option<String>>,
    /// `:qa` answered "quit anyway": do not save on the way out.
    pub discard_on_close: Cell<bool>,
    pub session: RefCell<Session>,
    /// The first note was named on the command line (not the session's).
    pub explicit_note: Cell<bool>,
    /// The session as last written, to skip unchanged saves.
    pub saved_session: RefCell<Option<Session>>,
    /// The open `F1` key list and its filtered model.
    pub help: RefCell<Option<(gtk::Window, gtk::FilterListModel)>>,
    pub t0: Instant,
    pub this: Weak<App>,
}

impl App {
    fn new(
        gapp: &gtk::Application,
        t0: Instant,
        paths: Paths,
        root: &Path,
        config: Config,
        note: Option<PathBuf>,
    ) -> anyhow::Result<Rc<Self>> {
        crate::theme::apply_fonts(&config);
        crate::theme::restyle_reading(&config);
        let guard = Guard::from_env();
        let (vault, events) = VaultHandle::open_deferred(root, &config, guard.clone())?;
        let key = state::vault_key(vault.root())?;
        if let Ok(mut k) = crate::CRASH_VAULT_KEY.lock() {
            *k = Some(key.clone());
        }
        let (session, session_warnings) =
            Session::load(&paths.state.join(&key).join("session.toml"));
        let root = vault.root().to_owned();
        let session_note = session
            .tabs
            .get(session.active)
            .and_then(|t| t.note.as_ref())
            .map(|id| root.join(id.filename()));
        let explicit = note.is_some();
        let initial = note.or(session_note);
        let first = initial.as_ref().map(|p| (p.clone(), vault.read_note(p)));
        let window = gtk::ApplicationWindow::builder()
            .application(gapp)
            .default_width(1280)
            .default_height(800)
            .build();
        let outer = gtk::Paned::new(gtk::Orientation::Horizontal);
        let inner = gtk::Paned::new(gtk::Orientation::Horizontal);
        let app = Rc::new_cyclic(|this: &Weak<App>| App {
            window: window.clone(),
            files: files::Files::new(this, &root, &session, &config),
            tabs: tabs::Tabs::new(this),
            status: status::Status::new(),
            vault,
            paths,
            guard,
            key,
            config: RefCell::new(config),
            index: RefCell::new(Index::default()),
            index_ready: Cell::new(false),
            outer,
            inner,
            panel: panel::Panel::new(this),
            finder: finder::Finder::new(this),
            ops: RefCell::new(std::collections::HashMap::new()),
            index_generation: Cell::new(0),
            switcher: RefCell::new(None),
            completion: editor::completion::Completion::new(),
            dialog: RefCell::new(None),
            discard_on_close: Cell::new(false),
            session: RefCell::new(session),
            help: RefCell::new(None),
            saved_session: RefCell::new(None),
            explicit_note: Cell::new(explicit),
            t0,
            this: this.clone(),
        });
        app.layout();
        registry::install(&app);
        Self::connect_window(&app);
        let tab = app
            .tabs
            .new_tab()
            .ok_or_else(|| anyhow::anyhow!("cannot create initial tab"))?;
        match first {
            Some((path, Ok(text))) => {
                let line = if explicit {
                    1
                } else {
                    app.session
                        .borrow()
                        .tabs
                        .get(app.session.borrow().active)
                        .map_or(1, |t| t.line)
                };
                tab.load(&app, &path, text, line);
            }
            Some((path, Err(e))) => {
                app.status
                    .error(&format!("cannot open {}: {e}", app.relative(&path)));
            }
            None => {}
        }
        app.update_title();
        for w in session_warnings {
            app.status.error(&format!("session: {}", w.0));
        }
        debug::probe_first_frame(&app);
        app.window.present();
        tab.view.grab_focus();
        Self::listen(&app, events);
        let weak = app.this.clone();
        glib::idle_add_local_once(move || {
            if let Some(app) = weak.upgrade() {
                app.start_services();
            }
        });
        Ok(app)
    }

    /// Window layout: files | (centre | right), status line underneath.
    fn layout(&self) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        self.outer.set_start_child(Some(&self.files.root));
        self.outer.set_resize_start_child(false);
        self.outer.set_shrink_start_child(false);
        self.inner.set_start_child(Some(&self.tabs.root));
        self.inner.set_end_child(Some(&self.panel.root));
        self.inner.set_resize_end_child(false);
        self.inner.set_shrink_end_child(false);
        self.outer.set_end_child(Some(&self.inner));
        self.outer.set_vexpand(true);
        column.append(&self.outer);
        column.append(&self.status.root);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&column));
        overlay.add_overlay(&self.finder.root);
        self.window.set_child(Some(&overlay));
        let session = self.session.borrow();
        self.files.root.set_visible(session.files_visible);
        self.panel.root.set_visible(session.right_visible);
        // Files 1 : centre 4 : right 1.5 of the default width, unless saved.
        let width = 1280.0_f64;
        let outer = session
            .panes
            .first()
            .copied()
            .unwrap_or_else(|| px(width / 6.5));
        let inner = session
            .panes
            .get(1)
            .copied()
            .unwrap_or_else(|| px((width - f64::from(outer)) * 4.0 / 5.5));
        self.outer.set_position(outer);
        self.inner.set_position(inner);
    }

    /// Everything the first frame does not need: listing, watcher, index.
    fn start_services(self: &Rc<Self>) {
        nib_vault::trace_span!("start_services");
        if let Err(e) = self.vault.start_watching() {
            self.status.error(&format!("watching: {e}"));
        }
        self.files.start(
            self,
            self.explicit_note.get()
                || self.session.borrow().folder.is_empty()
                    && self.session.borrow().cursor.is_empty(),
        );
        self.status.set_index("indexing…");
        // Debug builds can hold the index back to check the not-ready paths.
        let delay = std::env::var("NIB_TEST_INDEX_DELAY_MS")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .and_then(|v| v.parse::<u64>().ok());
        match delay {
            Some(ms) => {
                let vault = self.vault.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    vault.start_index();
                });
            }
            None => self.vault.start_index(),
        }
        let weak = self.this.clone();
        glib::idle_add_local_once(move || {
            if let Some(app) = weak.upgrade() {
                editor::load_language(&app);
                app.remember_vault();
                crate::session::restore(&app);
                crate::session::start_timer(&app);
            }
        });
    }

    fn listen(app: &Rc<Self>, events: async_channel::Receiver<VaultEvent>) {
        let weak = app.this.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = events.recv().await {
                let Some(app) = weak.upgrade() else { break };
                app.dispatch(event);
            }
        });
    }

    fn dispatch(self: &Rc<Self>, event: VaultEvent) {
        match event {
            VaultEvent::Listing { folder, entries } => {
                self.files.on_listing(self, &folder, &entries);
            }
            VaultEvent::Index(e) => self.on_index(e),
            VaultEvent::NoteChanged { path, text, fmt } => {
                self.tabs.external_change(self, &path, text, fmt);
            }
            VaultEvent::NoteInvalid { path } => self.tabs.external_invalid(self, &path),
            VaultEvent::NoteRemoved { path } => self.tabs.mark_removed(self, &path),
            VaultEvent::NoteRenamed { from, to } => self.tabs.follow_rename(self, &from, &to),
            VaultEvent::Search(e) => self.finder.on_search(self, e),
            VaultEvent::Walk(e) => self.finder.on_walk(self, e),
            VaultEvent::Op(e) => fileops::on_event(self, e),
            VaultEvent::Notice(n) => self.status.notice(&n.message),
        }
    }

    fn on_index(self: &Rc<Self>, event: IndexEvent) {
        match event {
            IndexEvent::Progress(done, total) => {
                self.status.set_index(&format!(
                    "indexing {}/{}",
                    status::group(done),
                    status::group(total)
                ));
            }
            IndexEvent::Ready(index) => {
                *self.index.borrow_mut() = *index;
                self.index_ready.set(true);
                debug::trace_since(self.t0, "index_ready");
                self.update_title();
            }
            IndexEvent::Upsert(meta) => self.index.borrow_mut().upsert(*meta),
            IndexEvent::Remove(id) => self.index.borrow_mut().remove(&id),
            IndexEvent::Renamed(old, new) => self.index.borrow_mut().rename(&old, new),
            IndexEvent::AttachmentUpsert(path) => self.index.borrow_mut().upsert_attachment(&path),
            IndexEvent::AttachmentRemove(path) => self.index.borrow_mut().remove_attachment(&path),
        }
        self.index_generation.set(self.index_generation.get() + 1);
        self.panel.queue(self);
        if self.index_ready.get() {
            let count = self.index.borrow().notes.len();
            self.status
                .set_index(&format!("{} notes", status::group(count)));
        }
    }

    /// Handle a further command line (or the first one's note options).
    /// A command line for the open window: a note opens in a new tab,
    /// `--new` and `--daily` act here. Another vault needs `--profile`.
    pub fn invocation(self: &Rc<Self>, inv: &nib_core::cli::Invocation, path: Option<PathBuf>) {
        if let Some(vault) = &inv.vault
            && vault.canonicalize().ok().as_deref() != Some(self.vault.root())
        {
            self.status.error(&format!(
                "{} is open here; a second vault needs nib --profile NAME",
                self.vault_name()
            ));
            return;
        }
        if let Some(path) = path {
            self.open_path(&path, true);
        }
        if let Some(title) = &inv.new {
            crate::notes::new_note(self, title);
        }
        if inv.daily {
            crate::notes::daily(self, "");
        }
    }

    /// Open a path from outside: a note in the current tab, or a folder in the column.
    pub fn open_path(self: &Rc<Self>, path: &Path, new_tab: bool) {
        // The folder canonical, the name as given: a symlinked note stays itself.
        let path = &match (
            path.parent().and_then(|p| p.canonicalize().ok()),
            path.file_name(),
        ) {
            (Some(dir), Some(name)) if !path.is_dir() => dir.join(name),
            _ => path.canonicalize().unwrap_or_else(|_| path.to_owned()),
        };
        if !path.starts_with(self.vault.root()) {
            self.status.error(&format!(
                "{} is outside this vault; a second vault needs nib --profile NAME",
                path.display()
            ));
            return;
        }
        if path.is_dir() {
            self.files.show(self, path);
            return;
        }
        self.open_note(path, new_tab, None);
    }

    /// Open a note: its tab if one shows it already, else a new tab or the
    /// current one (whose previous note goes on its history).
    pub fn open_note(self: &Rc<Self>, path: &Path, new_tab: bool, line: Option<u32>) {
        if let Some(i) = self.tabs.index_of(path) {
            self.tabs.select(i);
            let tab = self.tabs.current();
            if let Some(line) = line {
                tab.goto_line(line);
            }
            self.tabs.focus_current();
            return;
        }
        let current = self.tabs.current();
        // An empty untitled tab is used rather than left behind.
        let blank = current.path.borrow().is_none()
            && current.buffer.char_count() == 0
            && !current.buffer.is_modified();
        let tab = if new_tab && !blank {
            let Some(tab) = self.tabs.new_tab() else {
                return;
            };
            tab
        } else {
            current
        };
        self.load_into(&tab, path, line, true);
    }

    /// Show a note (or an image) in a given tab.
    pub fn load_into(
        self: &Rc<Self>,
        tab: &Rc<editor::Tab>,
        path: &Path,
        line: Option<u32>,
        history: bool,
    ) -> bool {
        let note = if crate::reading::is_image(path) {
            None
        } else {
            match self.vault.read_note(path) {
                Ok(n) => Some(n),
                Err(e) => {
                    self.status
                        .error(&format!("cannot open {}: {e}", self.relative(path)));
                    return false;
                }
            }
        };
        if !editor::save_if_modified(self, tab) {
            self.status
                .error("unsaved changes — :w saves them; open another tab to keep them");
            return false;
        }
        self.completion.close();
        let previous = tab.path.borrow().clone();
        if history
            && let Some(previous) = previous.filter(|p| p != path)
            && !tab.config.get()
        {
            let at = u32::try_from(tab.cursor_line()).unwrap_or(1);
            tab.back.borrow_mut().push((previous, at));
            tab.forward.borrow_mut().clear();
        }
        tab.config.set(false);
        match note {
            None => self.show_image(tab, path),
            Some(text) => {
                let was_image = tab.image.replace(false);
                tab.load(self, path, text, line.unwrap_or(1));
                if was_image {
                    crate::reading::set_mode(self, tab, crate::reading::Mode::Source);
                } else if tab.mode.get() != crate::reading::Mode::Source {
                    crate::reading::render(self, tab);
                }
            }
        }
        tab.refresh_label(self);
        if self.tabs.is_current(tab) {
            self.update_title();
            self.panel.queue(self);
            self.tabs.focus_current();
        }
        true
    }

    /// `Alt+Left` / `Alt+Right`: the tab's previous or next note, at its line.
    pub fn history(self: &Rc<Self>, back: bool) {
        let tab = self.tabs.current();
        let (from, to) = if back {
            (&tab.back, &tab.forward)
        } else {
            (&tab.forward, &tab.back)
        };
        let Some((path, line)) = from.borrow().last().cloned() else {
            self.status.notice(if back {
                "no earlier note in this tab"
            } else {
                "no later note in this tab"
            });
            return;
        };
        let previous = tab.path.borrow().clone();
        let at = u32::try_from(tab.cursor_line()).unwrap_or(1);
        if self.load_into(&tab, &path, Some(line), false) {
            from.borrow_mut().pop();
            if let Some(current) = previous {
                to.borrow_mut().push((current, at));
            }
        }
    }

    fn show_image(self: &Rc<Self>, tab: &Rc<editor::Tab>, path: &Path) {
        *tab.path.borrow_mut() = Some(path.to_owned());
        tab.read_only.set(true);
        tab.image.set(true);
        tab.buffer.set_text("");
        tab.buffer.set_modified(false);
        tab.view.set_editable(false);
        if tab.mode.get() == crate::reading::Mode::Reading {
            crate::reading::render(self, tab);
        } else {
            crate::reading::set_mode(self, tab, crate::reading::Mode::Reading);
        }
    }

    /// Vault-relative display path.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(self.vault.root())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// Note identity of an absolute path inside the vault.
    pub fn note_id(&self, path: &Path) -> Option<NoteId> {
        let rel = path.strip_prefix(self.vault.root()).ok()?;
        let rel = rel.to_str()?;
        rel.to_ascii_lowercase()
            .ends_with(".md")
            .then(|| NoteId::new(rel).ok())
            .flatten()
    }

    pub fn vault_name(&self) -> String {
        self.vault
            .root()
            .file_name()
            .map_or_else(|| "/".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn update_title(&self) {
        let tab = self.tabs.current();
        tab.refresh_label(self);
        let note = tab.title(self);
        self.window
            .set_title(Some(&format!("{note} — {} — nib", self.vault_name())));
        self.status
            .set_path(&tab.display_path(self), tab.buffer.is_modified());
    }

    /// The quick switcher's candidates, rebuilt only when the index or the
    /// recent list changed since the last build (typing never re-reads disk).
    pub fn switcher_candidates(&self) -> Vec<nib_core::fuzzy::Candidate> {
        let generation = self.index_generation.get();
        let recent = self.session.borrow().recent.clone();
        let mut cache = self.switcher.borrow_mut();
        if let Some((g, r, c)) = cache.as_ref()
            && *g == generation
            && *r == recent
        {
            return c.clone();
        }
        let candidates = nib_core::fuzzy::switcher_candidates(&self.index.borrow(), &recent);
        *cache = Some((generation, recent, candidates.clone()));
        candidates
    }

    /// Move a note to the front of the recent list (switcher order, `gr`).
    pub fn remember_recent(&self, id: &NoteId) {
        let mut session = self.session.borrow_mut();
        session.recent.retain(|r| r != id);
        session.recent.insert(0, id.clone());
        session.recent.truncate(64);
    }

    /// Write `vault.last` without disturbing the rest of the file's text.
    fn remember_vault(&self) {
        let root = self.vault.root().to_string_lossy().into_owned();
        if self.config.borrow().vault.last == root {
            return;
        }
        let file = self.paths.config.join("nib.toml");
        let Ok(text) = std::fs::read_to_string(&file) else {
            return;
        };
        let updated = set_vault_last(&text, &root);
        match nib_vault::fsx::save(&self.guard, &file, updated.as_bytes()) {
            Ok(_) => self.config.borrow_mut().vault.last = root,
            Err(e) => log::warn!("cannot remember the vault: {e}"),
        }
    }

    /// After the main loop: edits only (the session was saved at close,
    /// while the widgets still said what was visible).
    fn shutdown(&self) {
        if self.discard_on_close.get() || !self.config.borrow().editor.autosave {
            return;
        }
        for tab in self.tabs.all() {
            editor::save_if_modified(self, &tab);
        }
    }

    /// Save every modified note (autosave on window focus-out, P§16).
    pub fn autosave_all(&self) {
        if !self.config.borrow().editor.autosave {
            return;
        }
        for tab in self.tabs.all() {
            if !tab.config.get() {
                editor::save_if_modified(self, &tab);
            }
        }
    }

    fn connect_window(app: &Rc<Self>) {
        let weak = app.this.clone();
        app.window.connect_is_active_notify(move |w| {
            let Some(app) = weak.upgrade() else { return };
            if w.is_active() {
                // Watches ran out (ENOSPC): re-list what is shown on focus-in.
                if app.vault.needs_relist() {
                    app.files.reload(&app);
                }
            } else {
                app.autosave_all();
            }
        });
        let weak = app.this.clone();
        app.window.connect_close_request(move |_| {
            if let Some(app) = weak.upgrade() {
                if !app.discard_on_close.get() {
                    app.autosave_all();
                    let modified: Vec<String> = app
                        .tabs
                        .all()
                        .iter()
                        .filter(|t| t.buffer.is_modified())
                        .map(|t| t.display_path(&app))
                        .collect();
                    if !modified.is_empty() {
                        let question =
                            format!("Unsaved changes in {}. Quit anyway?", modified.join(", "));
                        crate::dialogs::ask(&app, &question, |app, yes| {
                            if yes {
                                app.discard_on_close.set(true);
                                app.window.close();
                            }
                        });
                        return glib::Propagation::Stop;
                    }
                }
                crate::session::save(&app);
                // Vim keeps marks in the buffers; let go before the views go.
                app.completion.close();
                for tab in app.tabs.all() {
                    editor::vim::detach(&tab);
                }
            }
            glib::Propagation::Proceed
        });
    }
}

/// A single click activates the row it lands on, without the hover
/// selection `set_single_click_activate` brings (rows would follow the pointer).
pub fn click_activates(list: &gtk::ListView) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_released(|gesture, n, _, _| {
        let Some(list) = gesture.widget().and_downcast::<gtk::ListView>() else {
            return;
        };
        let selected = list
            .model()
            .and_downcast::<gtk::SingleSelection>()
            .map(|s| s.selected());
        if n == 1
            && let Some(pos) = selected.filter(|p| *p != gtk::INVALID_LIST_POSITION)
        {
            list.emit_by_name::<()>("activate", &[&pos]);
        }
    });
    list.add_controller(click);
}

/// Pixels from a computed length (rounded; lengths here are far inside `i32`).
#[allow(clippy::cast_possible_truncation)]
pub fn px(v: f64) -> i32 {
    v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Replace or insert `last = "…"` in the `[vault]` table, keeping every other byte.
pub fn set_vault_last(text: &str, root: &str) -> String {
    let quoted = toml::Value::String(root.to_owned()).to_string();
    let line = format!("last = {quoted}");
    let mut out = String::with_capacity(text.len() + line.len() + 8);
    let mut table = String::new();
    let mut done = false;
    for l in text.split_inclusive('\n') {
        let trimmed = l.trim();
        if trimmed.starts_with('[') {
            if table == "[vault]" && !done {
                out.push_str(&line);
                out.push('\n');
                done = true;
            }
            trimmed.clone_into(&mut table);
        } else if table == "[vault]" && !done && trimmed.starts_with("last") {
            let key = trimmed.split('=').next().unwrap_or_default().trim();
            if key == "last" {
                out.push_str(&line);
                out.push('\n');
                done = true;
                continue;
            }
        }
        out.push_str(l);
    }
    if !done {
        if table == "[vault]" {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&line);
            out.push('\n');
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("[vault]\n");
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::set_vault_last;
    #[test]
    fn vault_last_keeps_other_text() {
        let t = "# c\n[vault]\nlast = \"\"\nnew_folder = \"\"\n[editor]\nvim = true\n";
        assert_eq!(
            set_vault_last(t, "/v"),
            "# c\n[vault]\nlast = \"/v\"\nnew_folder = \"\"\n[editor]\nvim = true\n"
        );
        assert_eq!(
            set_vault_last("[vault]\nx = 1\n[editor]\n", "/v"),
            "[vault]\nx = 1\nlast = \"/v\"\n[editor]\n"
        );
        assert_eq!(set_vault_last("", "/v"), "[vault]\nlast = \"/v\"\n");
        assert_eq!(
            set_vault_last("[vault]\nlast_x = 1", "/v"),
            "[vault]\nlast_x = 1\nlast = \"/v\"\n"
        );
    }
}
