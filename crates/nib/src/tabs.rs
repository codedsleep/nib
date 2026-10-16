//! The open tabs (P§15) and how external changes reach them (P§16).
use crate::{
    app::App,
    editor::{Tab, vim},
    gtk,
};
use gtk::prelude::*;
use nib_core::text::TextFormat;
use nib_vault::NoteText;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::{Rc, Weak},
};

/// Tabs beyond this close their least recently used unmodified one (P§15).
const MAX_TABS: usize = 32;

pub struct Tabs {
    /// Tab bar above the page stack.
    pub root: gtk::Box,
    bar: gtk::Box,
    stack: gtk::Stack,
    list: RefCell<Vec<Rc<Tab>>>,
    active: Cell<usize>,
    next_id: Cell<u64>,
    /// A clock for least-recently-used: bumped by every switch.
    clock: Cell<u64>,
    app: Weak<App>,
}

impl Tabs {
    pub fn new(app: &Weak<App>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bar.add_css_class("nib-tabbar");
        let stack = gtk::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.set_vexpand(true);
        stack.set_hexpand(true);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&bar)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hscrollbar_policy(gtk::PolicyType::External)
            .build();
        root.append(&scroll);
        root.append(&stack);
        Self {
            root,
            bar,
            stack,
            list: RefCell::new(Vec::new()),
            active: Cell::new(0),
            next_id: Cell::new(1),
            clock: Cell::new(0),
            app: app.clone(),
        }
    }

    /// A new empty tab at the end, made current.
    pub fn new_tab(&self) -> Rc<Tab> {
        let tab = self.add_tab();
        self.select(self.list.borrow().len() - 1);
        tab
    }

    /// A new empty tab at the end, not selected (session restore).
    pub fn add_tab(&self) -> Rc<Tab> {
        let app = self.app.upgrade().expect("tabs are created by a live App");
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let tab = Tab::new(&app, id);
        self.stack.add_child(&tab.root);
        if let Some(first) = self.list.borrow().first() {
            tab.button.set_group(Some(&first.button));
        }
        self.bar.append(&tab.button);
        let weak = self.app.clone();
        tab.button.connect_toggled(move |b| {
            if !b.is_active() {
                return;
            }
            let Some(app) = weak.upgrade() else { return };
            let index = app.tabs.list.borrow().iter().position(|t| t.id == id);
            if let Some(i) = index.filter(|i| *i != app.tabs.active.get()) {
                glib::idle_add_local_once(move || app.tabs.select(i));
            }
        });
        let middle = gtk::GestureClick::new();
        middle.set_button(2);
        let weak = self.app.clone();
        middle.connect_released(move |_, _, _, _| {
            let Some(app) = weak.upgrade() else { return };
            if let Some(tab) = app.tabs.by_id(id) {
                crate::commands::close_tab(&app, &tab);
            }
        });
        tab.button.add_controller(middle);
        self.list.borrow_mut().push(tab.clone());
        tab.refresh_label(&app);
        self.limit(&app);
        tab
    }

    /// Over 32 tabs, the least recently used unmodified one closes.
    fn limit(&self, app: &Rc<App>) {
        if self.list.borrow().len() <= MAX_TABS {
            return;
        }
        let active = self.current().id;
        let oldest = self
            .list
            .borrow()
            .iter()
            .filter(|t| t.id != active && !t.buffer.is_modified())
            .min_by_key(|t| t.used.get())
            .cloned();
        if let Some(t) = oldest {
            self.close(app, &t);
        }
    }

    pub fn current(&self) -> Rc<Tab> {
        let list = self.list.borrow();
        list[self.active.get().min(list.len() - 1)].clone()
    }

    pub fn all(&self) -> Vec<Rc<Tab>> {
        self.list.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.list.borrow().len()
    }

    pub fn is_current(&self, tab: &Tab) -> bool {
        self.is_current_id(tab.id)
    }

    pub fn is_current_id(&self, id: u64) -> bool {
        self.list
            .borrow()
            .get(self.active.get())
            .is_some_and(|t| t.id == id)
    }

    pub fn active_index(&self) -> usize {
        self.active.get()
    }

    pub fn select(&self, index: usize) {
        let tab = {
            let list = self.list.borrow();
            let Some(tab) = list.get(index) else { return };
            tab.clone()
        };
        let previous = self.list.borrow().get(self.active.get()).cloned();
        self.active.set(index);
        self.clock.set(self.clock.get() + 1);
        tab.used.set(self.clock.get());
        self.stack.set_visible_child(&tab.root);
        if !tab.button.is_active() {
            tab.button.set_active(true);
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        // Autosave on tab switch (P§16).
        if let Some(previous) = previous.filter(|p| p.id != tab.id)
            && app.config.borrow().editor.autosave
        {
            crate::editor::save_if_modified(&app, &previous);
        }
        app.completion.close();
        app.update_title();
        tab.show_position(&app);
        vim::show(&app, &tab);
        app.status.set_words(tab.word_count.get());
        if let Some(words) = tab.words.borrow().as_ref() {
            words.poke();
        }
        app.panel.queue(&app);
    }

    /// Focus the current tab's view (source or reading).
    pub fn focus_current(&self) {
        let tab = self.current();
        let reading = tab.reading.borrow().clone();
        match (tab.mode.get(), reading) {
            (crate::reading::Mode::Reading, Some(r)) => {
                r.view.grab_focus();
            }
            _ => {
                tab.view.grab_focus();
            }
        }
    }

    /// `Alt+[` / `Alt+]`, `:tab prev|next`.
    pub fn cycle(&self, delta: i64) {
        let n = i64::try_from(self.len()).unwrap_or(1);
        let at = i64::try_from(self.active.get()).unwrap_or(0);
        self.select(usize::try_from((at + delta).rem_euclid(n)).unwrap_or(0));
        self.focus_current();
    }

    /// Close a tab (its edits are the caller's business). The last tab is
    /// replaced by an empty one: the window stays.
    pub fn close(&self, app: &Rc<App>, tab: &Rc<Tab>) {
        let Some(index) = self.list.borrow().iter().position(|t| t.id == tab.id) else {
            return;
        };
        app.completion.close();
        let focused = tab.view.has_focus()
            || tab
                .reading
                .borrow()
                .as_ref()
                .is_some_and(|r| r.view.has_focus());
        if self.list.borrow().len() == 1 {
            self.new_tab();
        }
        let current = self.active.get();
        self.list.borrow_mut().remove(index);
        vim::detach(tab);
        app.completion.release(&tab.view);
        self.bar.remove(&tab.button);
        self.stack.remove(&tab.root);
        let len = self.list.borrow().len();
        let next = if current > index || current >= len {
            current.saturating_sub(1)
        } else {
            current
        };
        self.select(next.min(len - 1));
        if focused {
            self.focus_current();
        }
    }

    /// Put the tabs (and their buttons) in the order of these ids.
    pub fn reorder(&self, ids: &[u64]) {
        let active = self.current().id;
        self.list
            .borrow_mut()
            .sort_by_key(|t| ids.iter().position(|i| *i == t.id).unwrap_or(usize::MAX));
        for tab in self.list.borrow().iter() {
            self.bar.remove(&tab.button);
            self.bar.append(&tab.button);
        }
        if let Some(i) = self.list.borrow().iter().position(|t| t.id == active) {
            self.active.set(i);
        }
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.list
            .borrow()
            .iter()
            .position(|t| t.path.borrow().as_deref() == Some(path))
    }

    pub fn by_id(&self, id: u64) -> Option<Rc<Tab>> {
        self.list.borrow().iter().find(|t| t.id == id).cloned()
    }

    pub fn find(&self, path: &Path) -> Option<Rc<Tab>> {
        self.list
            .borrow()
            .iter()
            .find(|t| t.path.borrow().as_deref() == Some(path))
            .cloned()
    }

    /// Another program changed an open note: reload an unmodified buffer in
    /// place (cursor kept by line), never touch a modified one.
    pub fn external_change(&self, app: &Rc<App>, path: &Path, text: String, fmt: TextFormat) {
        let Some(tab) = self.find(path) else { return };
        let rel = app.relative(path);
        if tab.buffer.is_modified() {
            tab.disk_changed.set(true);
            app.status.notice(&format!(
                "{rel} changed on disk — :e! to reload, :w to overwrite"
            ));
            return;
        }
        let line = tab.buffer.iter_at_mark(&tab.buffer.get_insert()).line();
        let note = NoteText {
            text,
            fmt,
            read_only: false,
            notice: None,
        };
        tab.load(app, path, note, u32::try_from(line + 1).unwrap_or(1));
        app.status.notice(&format!("reloaded {rel}"));
    }

    pub fn external_invalid(&self, app: &Rc<App>, path: &Path) {
        let Some(tab) = self.find(path) else { return };
        let rel = app.relative(path);
        if tab.buffer.is_modified() {
            tab.disk_changed.set(true);
            app.status.notice(&format!(
                "{rel} is no longer UTF-8 on disk; :w would overwrite it"
            ));
            return;
        }
        if let Ok(note) = app.vault.read_note(path) {
            tab.load(app, path, note, 1);
        }
        app.status
            .notice(&format!("{rel} is no longer UTF-8; read-only"));
    }

    pub fn mark_removed(&self, app: &Rc<App>, path: &Path) {
        let Some(tab) = self.find(path) else { return };
        tab.removed.set(true);
        app.status.notice(&format!(
            "{} was removed on disk — :w recreates it",
            app.relative(path)
        ));
    }

    /// Follow a rename without touching the buffer or its undo history.
    pub fn follow_rename(&self, app: &Rc<App>, from: &Path, to: &Path) {
        for tab in self.list.borrow().iter() {
            let renamed = {
                let path = tab.path.borrow();
                path.as_deref().and_then(|p| {
                    p.strip_prefix(from).ok().map(|rest| {
                        if rest.as_os_str().is_empty() {
                            to.to_owned()
                        } else {
                            to.join(rest)
                        }
                    })
                })
            };
            if let Some(new) = renamed {
                *tab.path.borrow_mut() = Some(new);
            }
        }
        for tab in self.list.borrow().iter() {
            tab.refresh_label(app);
        }
        app.update_title();
    }
}
