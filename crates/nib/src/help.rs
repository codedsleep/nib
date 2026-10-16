//! `F1` (and `~` in the files column): every registry entry with its keys,
//! filtered as you type. Built from the registry, never hand-written.
use crate::{app::App, gtk, registry};
use gtk::prelude::*;
use std::rc::Rc;

/// One line per entry: keys, label, and where the keys are live.
pub fn rows() -> Vec<String> {
    registry::ENTRIES
        .iter()
        .map(|e| {
            let keys: Vec<String> = e
                .keys
                .iter()
                .map(|k| registry::display_key(e.scope, k))
                .collect();
            let scope = match e.scope {
                registry::Scope::Window | registry::Scope::Command => "",
                registry::Scope::Editor => "  (editor)",
                registry::Scope::Files => "  (files)",
            };
            let commands: Vec<String> = e.commands.iter().map(|c| format!(":{c}")).collect();
            format!(
                "{:<24} {:<12} {}{scope}",
                keys.join(" "),
                commands.join(" "),
                e.label
            )
        })
        .collect()
}

pub fn show(app: &Rc<App>) {
    let window = gtk::Window::builder()
        .transient_for(&app.window)
        .title("nib keys")
        .default_width(720)
        .default_height(560)
        .build();
    window.add_css_class("nib-help");
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some("filter"));
    let all = rows();
    let strings = gtk::StringList::new(&all.iter().map(String::as_str).collect::<Vec<_>>());
    let filter = gtk::StringFilter::new(Some(gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<&gtk::Expression>,
        "string",
    )));
    filter.set_ignore_case(true);
    filter.set_match_mode(gtk::StringFilterMatchMode::Substring);
    let model = gtk::FilterListModel::new(Some(strings), Some(filter.clone()));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.add_css_class("monospace");
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(label), Some(text)) = (
            item.child().and_downcast::<gtk::Label>(),
            item.item().and_downcast::<gtk::StringObject>(),
        ) else {
            return;
        };
        label.set_text(&text.string());
    });
    let list = gtk::ListView::new(
        Some(gtk::NoSelection::new(Some(model.clone()))),
        Some(factory),
    );
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.add_css_class("nib-dialog");
    column.append(&entry);
    column.append(&scroller);
    window.set_child(Some(&column));
    entry.connect_search_changed(move |e| filter.set_search(Some(&e.text())));
    let w = window.clone();
    entry.connect_stop_search(move |_| w.close());
    let keys = gtk::EventControllerKey::new();
    let w = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            w.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
    *app.help.borrow_mut() = Some((window.clone(), model));
    let weak = app.this.clone();
    window.connect_close_request(move |_| {
        if let Some(app) = weak.upgrade() {
            app.help.borrow_mut().take();
        }
        glib::Propagation::Proceed
    });
    window.present();
    entry.grab_focus();
}

/// Rows shown by an open key list, for `--test-dump`.
pub fn shown(app: &App) -> Option<u32> {
    app.help.borrow().as_ref().map(|(_, m)| m.n_items())
}
