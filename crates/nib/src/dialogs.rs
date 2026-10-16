//! The only two dialog kinds (P§7): a question answered with a key, and a
//! one-line entry. Both are transient windows, which float under Hyprland
//! without window rules.
use crate::{app::App, gtk};
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

type Answer = Box<dyn FnOnce(&Rc<App>, Option<char>)>;
type Text = Box<dyn FnOnce(&Rc<App>, Option<String>)>;

fn window(app: &App, title: &str) -> gtk::Window {
    let window = gtk::Window::builder()
        .transient_for(&app.window)
        .modal(true)
        .title(title)
        .resizable(false)
        .default_width(420)
        .build();
    window.add_css_class("nib-dialog-window");
    window
}

/// Ask a question; each `(key, label)` answers it, `Esc` answers `None`.
/// The first choice is also `Enter`.
pub fn choose(
    app: &Rc<App>,
    question: &str,
    choices: &[(char, &str)],
    on_answer: impl FnOnce(&Rc<App>, Option<char>) + 'static,
) {
    let window = window(app, "nib");
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.add_css_class("nib-dialog");
    let label = gtk::Label::new(Some(question));
    label.set_wrap(true);
    label.set_xalign(0.0);
    let hint: Vec<String> = choices.iter().map(|(k, l)| format!("[{k}] {l}")).collect();
    let keys = gtk::Label::new(Some(&format!("{}   [Esc] cancel", hint.join("   "))));
    keys.add_css_class("dim");
    keys.set_xalign(0.0);
    column.append(&label);
    column.append(&keys);
    window.set_child(Some(&column));
    let callback: Rc<RefCell<Option<Answer>>> = Rc::new(RefCell::new(Some(Box::new(on_answer))));
    let choices: Vec<char> = choices.iter().map(|(k, _)| *k).collect();
    let controller = gtk::EventControllerKey::new();
    let weak = app.this.clone();
    let w = window.clone();
    let cb = callback.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        let answer = match key {
            gtk::gdk::Key::Escape => None,
            gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => choices.first().copied(),
            k => match k.to_unicode().map(|c| c.to_ascii_lowercase()) {
                Some(c) if choices.contains(&c) => Some(c),
                _ => return glib::Propagation::Stop,
            },
        };
        finish(&weak, &w, &cb, answer);
        glib::Propagation::Stop
    });
    window.add_controller(controller);
    let weak = app.this.clone();
    window.connect_close_request(move |w| {
        finish(&weak, w, &callback, None);
        glib::Propagation::Proceed
    });
    *app.dialog.borrow_mut() = Some(question.to_owned());
    window.present();
}

fn finish(
    weak: &std::rc::Weak<App>,
    window: &gtk::Window,
    callback: &Rc<RefCell<Option<Answer>>>,
    answer: Option<char>,
) {
    let Some(cb) = callback.borrow_mut().take() else {
        return;
    };
    window.close();
    if let Some(app) = weak.upgrade() {
        app.dialog.borrow_mut().take();
        cb(&app, answer);
    }
}

/// A yes/no question: `y`/`Enter` and `n`/`Esc`.
pub fn ask(app: &Rc<App>, question: &str, on_answer: impl FnOnce(&Rc<App>, bool) + 'static) {
    choose(
        app,
        question,
        &[('y', "yes"), ('n', "no")],
        move |app, a| {
            on_answer(app, a == Some('y'));
        },
    );
}

/// A one-line entry: `Enter` accepts, `Esc` cancels.
pub fn prompt(
    app: &Rc<App>,
    question: &str,
    initial: &str,
    on_text: impl FnOnce(&Rc<App>, Option<String>) + 'static,
) {
    let window = window(app, "nib");
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.add_css_class("nib-dialog");
    let label = gtk::Label::new(Some(question));
    label.set_xalign(0.0);
    let entry = gtk::Entry::new();
    entry.set_text(initial);
    column.append(&label);
    column.append(&entry);
    window.set_child(Some(&column));
    let callback: Rc<RefCell<Option<Text>>> = Rc::new(RefCell::new(Some(Box::new(on_text))));
    let done = {
        let weak = app.this.clone();
        let window = window.clone();
        let callback = callback.clone();
        move |text: Option<String>| {
            let Some(cb) = callback.borrow_mut().take() else {
                return;
            };
            window.close();
            if let Some(app) = weak.upgrade() {
                app.dialog.borrow_mut().take();
                cb(&app, text);
            }
        }
    };
    let d = done.clone();
    entry.connect_activate(move |e| d(Some(e.text().into())));
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let d = done.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            d(None);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
    window.connect_close_request(move |_| {
        done(None);
        glib::Propagation::Proceed
    });
    *app.dialog.borrow_mut() = Some(question.to_owned());
    window.present();
    entry.grab_focus();
    // Select the stem so typing replaces the name but keeps `.md`.
    let stem = initial
        .rfind('.')
        .filter(|i| *i > 0)
        .unwrap_or(initial.len());
    entry.select_region(
        0,
        i32::try_from(initial[..stem].chars().count()).unwrap_or(-1),
    );
}
