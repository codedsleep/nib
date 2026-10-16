//! `VimIMContext` per view (P§10): the mode and pending keys in the status
//! line, and the `:` command bar routed through the registry.
use super::Tab;
use crate::{app::App, commands, gtk};
use gtk::prelude::*;
use std::rc::Rc;

thread_local! {
    /// Every form GtkSourceView's insert-mode bar text can take (G3). Its
    /// vim states translate with `<glib/gi18n.h>`'s `_()`, i.e. in the
    /// application's text domain, so in nib the bar reads `-- INSERT --` in
    /// every locale today; the library's own translation is accepted too in
    /// case that is fixed.
    static INSERT: [String; 3] = [
        "-- INSERT --".into(),
        glib::dgettext(Some("gtksourceview-5"), "-- INSERT --").into(),
        glib::dgettext(None, "-- INSERT --").into(),
    ];
}

/// The vim context and the key controller that feeds it.
pub struct Vim {
    pub context: sourceview5::VimIMContext,
    controller: gtk::EventControllerKey,
}

/// Whether typed text goes into the buffer: vim's insert mode, or no vim.
pub fn inserting(tab: &Tab) -> bool {
    match tab.vim.borrow().as_ref() {
        None => true,
        Some(v) => {
            let bar = v.context.command_bar_text();
            INSERT.with(|forms| forms.iter().any(|f| bar == f.as_str()))
        }
    }
}

/// Slot 1 of the status line for a tab: the bar text, `NOR` when vim shows none.
pub fn mode_text(tab: &Tab) -> String {
    match tab.vim.borrow().as_ref() {
        None => String::new(),
        Some(v) => {
            let bar = v.context.command_bar_text();
            if bar.is_empty() {
                "NOR".into()
            } else {
                bar.into()
            }
        }
    }
}

pub fn pending_text(tab: &Tab) -> String {
    tab.vim
        .borrow()
        .as_ref()
        .map(|v| v.context.command_text().into())
        .unwrap_or_default()
}

/// Attach or detach vim to follow `editor.vim`.
pub fn sync(app: &Rc<App>, tab: &Rc<Tab>) {
    let wanted = app.config.borrow().editor.vim;
    let has = tab.vim.borrow().is_some();
    if wanted && !has {
        attach(app, tab);
    } else if !wanted && has {
        detach(tab);
        show(app, tab);
    }
}

/// Let go of the view (`editor.vim` turned off, or the tab closing).
pub fn detach(tab: &Tab) {
    if let Some(v) = tab.vim.borrow_mut().take() {
        v.context.set_client_widget(None::<&gtk::Widget>);
        tab.view.remove_controller(&v.controller);
    }
}

fn attach(app: &Rc<App>, tab: &Rc<Tab>) {
    let context = sourceview5::VimIMContext::new();
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.set_im_context(Some(&context));
    context.set_client_widget(Some(&tab.view));
    tab.view.add_controller(controller.clone());
    // GTK runs the most recently added controller first: nib's goes on top again.
    tab.view.remove_controller(&tab.keys);
    tab.view.add_controller(tab.keys.clone());
    let weak = app.this.clone();
    let tab_weak = Rc::downgrade(tab);
    let refresh = move || {
        let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
            return;
        };
        if app.tabs.is_current(&tab) {
            show(&app, &tab);
        }
    };
    let r = refresh.clone();
    context.connect_command_bar_text_notify(move |_| r());
    context.connect_command_text_notify(move |_| refresh());
    let weak = app.this.clone();
    context.connect_execute_command(move |_, line| {
        weak.upgrade()
            .is_some_and(|app| commands::execute(&app, line))
    });
    // Spike evidence (6.1): nib claims `:w` in execute-command, so vim's own
    // `write` signal must never fire. Debug builds say so if it does.
    #[cfg(debug_assertions)]
    context.connect_write(|_, _, _| eprintln!("vim write signal"));
    *tab.vim.borrow_mut() = Some(Vim {
        context,
        controller,
    });
    show(app, tab);
}

/// Put a tab's vim state into the status line.
pub fn show(app: &App, tab: &Tab) {
    app.status.set_mode(&mode_text(tab));
    if !app.files.has_focus() {
        app.status.set_pending(&pending_text(tab));
    }
}
