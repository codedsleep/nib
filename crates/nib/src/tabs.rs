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

pub struct Tabs {
    /// Tab bar above the page stack.
    pub root: gtk::Box,
    stack: gtk::Stack,
    list: RefCell<Vec<Rc<Tab>>>,
    active: Cell<usize>,
    next_id: Cell<u64>,
    app: Weak<App>,
}

impl Tabs {
    pub fn new(app: &Weak<App>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bar.add_css_class("nib-tabbar");
        bar.set_visible(false);
        let stack = gtk::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.set_vexpand(true);
        stack.set_hexpand(true);
        root.append(&bar);
        root.append(&stack);
        Self {
            root,
            stack,
            list: RefCell::new(Vec::new()),
            active: Cell::new(0),
            next_id: Cell::new(1),
            app: app.clone(),
        }
    }

    /// A new empty tab, made current.
    pub fn new_tab(&self) -> Rc<Tab> {
        let app = self.app.upgrade().expect("tabs are created by a live App");
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let tab = Tab::new(&app, id);
        self.stack.add_child(&tab.root);
        self.list.borrow_mut().push(tab.clone());
        self.select(self.list.borrow().len() - 1);
        tab
    }

    pub fn current(&self) -> Rc<Tab> {
        let list = self.list.borrow();
        list[self.active.get().min(list.len() - 1)].clone()
    }

    pub fn all(&self) -> Vec<Rc<Tab>> {
        self.list.borrow().clone()
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
        self.active.set(index);
        self.stack.set_visible_child(&tab.root);
        if let Some(app) = self.app.upgrade() {
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
    }

    /// Close a tab (its edits are the caller's business). The last tab is
    /// replaced by an empty one: the window stays.
    pub fn close(&self, app: &Rc<App>, tab: &Rc<Tab>) {
        let Some(index) = self.list.borrow().iter().position(|t| t.id == tab.id) else {
            return;
        };
        app.completion.close();
        let focused = tab.view.has_focus();
        if self.list.borrow().len() == 1 {
            self.new_tab();
        }
        let current = self.active.get();
        self.list.borrow_mut().remove(index);
        vim::detach(tab);
        app.completion.release(&tab.view);
        self.stack.remove(&tab.root);
        let len = self.list.borrow().len();
        let next = if current > index || current >= len {
            current.saturating_sub(1)
        } else {
            current
        };
        self.select(next.min(len - 1));
        if focused {
            self.current().view.grab_focus();
        }
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
        app.update_title();
    }
}
