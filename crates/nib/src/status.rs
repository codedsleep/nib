//! The one-row status line (P§7): mode or vim command bar, pending keys,
//! note path, word count, `line:col`, and the index status or a notice.
use crate::gtk;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

const NOTICE: Duration = Duration::from_secs(4);

pub struct Status {
    pub root: gtk::Box,
    mode: gtk::Label,
    pending: gtk::Label,
    path: gtk::Label,
    words: gtk::Label,
    position: gtk::Label,
    right: gtk::Label,
    index: Rc<RefCell<String>>,
    /// Bumped by every notice; a notice's timeout restores the index text
    /// only if no later notice replaced it.
    serial: Rc<Cell<u64>>,
    showing: Rc<Cell<bool>>,
}

fn label(class: &str) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.add_css_class(class);
    l.set_single_line_mode(true);
    l
}

impl Status {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("nib-status");
        let mode = label("mode");
        mode.set_xalign(0.0);
        let pending = label("dim");
        let path = label("path");
        path.set_hexpand(true);
        path.set_xalign(0.0);
        path.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        let words = label("dim");
        let position = label("dim");
        let right = label("dim");
        right.set_ellipsize(gtk::pango::EllipsizeMode::End);
        right.set_max_width_chars(60);
        for w in [&mode, &pending, &path, &words, &position, &right] {
            root.append(w);
        }
        Self {
            root,
            mode,
            pending,
            path,
            words,
            position,
            right,
            index: Rc::new(RefCell::new(String::new())),
            serial: Rc::new(Cell::new(0)),
            showing: Rc::new(Cell::new(false)),
        }
    }

    /// Slot 1: the vim command bar (`-- INSERT --`, `:w`), `NOR` when empty.
    pub fn set_mode(&self, text: &str) {
        self.mode.set_text(text);
    }

    /// Slot 2: keys of a started sequence.
    pub fn set_pending(&self, text: &str) {
        self.pending.set_text(text);
    }

    pub fn set_path(&self, path: &str, modified: bool) {
        if modified {
            self.path.set_text(&format!("{path} ●"));
        } else {
            self.path.set_text(path);
        }
    }

    pub fn set_words(&self, words: Option<usize>) {
        match words {
            Some(1) => self.words.set_text("1 word"),
            Some(n) => self.words.set_text(&format!("{} words", group(n))),
            None => self.words.set_text(""),
        }
    }

    pub fn set_position(&self, line: i32, column: i32) {
        self.position.set_text(&format!("{line}:{column}"));
    }

    /// Index state, shown whenever no notice is.
    pub fn set_index(&self, text: &str) {
        text.clone_into(&mut self.index.borrow_mut());
        if !self.showing.get() {
            self.right.set_text(text);
        }
    }

    /// A message in the right slot for four seconds, also logged.
    pub fn notice(&self, message: &str) {
        log::info!("notice: {message}");
        self.show(message, "notice");
    }

    /// A failure in the right slot for four seconds, also logged.
    pub fn error(&self, message: &str) {
        log::warn!("{message}");
        self.show(message, "error");
    }

    fn show(&self, message: &str, class: &str) {
        self.right.set_css_classes(&[class]);
        self.right.set_text(message);
        self.right.set_tooltip_text(Some(message));
        let serial = self.serial.get() + 1;
        self.serial.set(serial);
        self.showing.set(true);
        let (right, index, current, showing) = (
            self.right.clone(),
            self.index.clone(),
            self.serial.clone(),
            self.showing.clone(),
        );
        glib::timeout_add_local_once(NOTICE, move || {
            if current.get() == serial {
                showing.set(false);
                right.set_css_classes(&["dim"]);
                right.set_tooltip_text(None);
                right.set_text(&index.borrow());
            }
        });
    }

    /// Texts of every slot, for `--test-dump`.
    pub fn texts(&self) -> [String; 6] {
        [
            self.mode.text().into(),
            self.pending.text().into(),
            self.path.text().into(),
            self.words.text().into(),
            self.position.text().into(),
            self.right.text().into(),
        ]
    }
}

/// `3041` → `3 041`, grouping thousands with a space.
pub fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn group() {
        assert_eq!(super::group(0), "0");
        assert_eq!(super::group(999), "999");
        assert_eq!(super::group(3041), "3 041");
        assert_eq!(super::group(1_234_567), "1 234 567");
    }
}
