//! Editing behaviours (P§9, S4): `nib-core::edit` decides, this file reads
//! the current line, applies the edit and puts the cursor back. The key
//! controller here is added before vim's, so it sees every key first and
//! passes on whatever it does not claim.
use super::{Tab, completion, vim};
use crate::{app::App, gtk, launch, registry};
use gtk::prelude::*;
use nib_core::edit::{self, TextEdit};
use std::rc::Rc;

/// The current line as text, its start iter, and the cursor's byte column.
fn current_line(tab: &Tab) -> (String, gtk::TextIter, usize) {
    let cursor = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
    let mut start = cursor;
    start.set_line_offset(0);
    let mut end = cursor;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let before = tab.buffer.text(&start, &cursor, true);
    let line = tab.buffer.text(&start, &end, true);
    (line.into(), start, before.len())
}

/// Apply a core edit whose byte offsets are relative to `base` (the text
/// that begins at `origin`), as one user action.
fn apply(tab: &Tab, base: &str, origin: &gtk::TextIter, e: &TextEdit) {
    let chars = |b: usize| {
        i32::try_from(
            base.get(..b.min(base.len()))
                .unwrap_or_default()
                .chars()
                .count(),
        )
        .unwrap_or(0)
    };
    // Iterators die with the first edit: keep plain offsets.
    let origin = origin.offset();
    let start_off = origin + chars(e.range.start);
    let end_off = origin + chars(e.range.end);
    let buffer = &tab.buffer;
    buffer.begin_user_action();
    let mut start = buffer.iter_at_offset(start_off);
    let mut end = buffer.iter_at_offset(end_off);
    buffer.delete(&mut start, &mut end);
    let mut at = buffer.iter_at_offset(start_off);
    buffer.insert(&mut at, &e.replacement);
    // The cursor offset is in the edited text: the prefix before the range is
    // unchanged, then the replacement, then the rest after the range.
    let count = |t: &str| i32::try_from(t.chars().count()).unwrap_or(0);
    let replaced = e.range.start + e.replacement.len();
    let cursor_chars = if e.cursor <= e.range.start {
        chars(e.cursor)
    } else if e.cursor <= replaced {
        chars(e.range.start)
            + count(
                e.replacement
                    .get(..e.cursor - e.range.start)
                    .unwrap_or_default(),
            )
    } else {
        let after = (e.range.end + (e.cursor - replaced)).min(base.len());
        chars(e.range.start)
            + count(&e.replacement)
            + count(base.get(e.range.end..after).unwrap_or_default())
    };
    buffer.place_cursor(&buffer.iter_at_offset(origin + cursor_chars));
    buffer.end_user_action();
    tab.view.scroll_mark_onscreen(&buffer.get_insert());
}

/// The key controller every tab gets; `vim::attach` re-adds it after vim's,
/// because GTK runs a widget's most recently added controller first.
pub fn controller(app: &Rc<App>, tab: &std::rc::Weak<Tab>) -> gtk::EventControllerKey {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = app.this.clone();
    let tab_weak = tab.clone();
    keys.connect_key_pressed(move |_, keyval, _, state| {
        let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
            return glib::Propagation::Proceed;
        };
        on_key(&app, &tab, keyval, state)
    });
    keys
}

pub fn install(app: &Rc<App>, tab: &Rc<Tab>) {
    tab.view.add_controller(tab.keys.clone());
    crate::attach::drop_target(app, &tab.view);
    // Ctrl+click follows the link under the pointer.
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = app.this.clone();
    let tab_weak = Rc::downgrade(tab);
    click.connect_pressed(move |gesture, _, x, y| {
        if !gesture
            .current_event_state()
            .contains(gtk::gdk::ModifierType::CONTROL_MASK)
        {
            return;
        }
        let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        let (bx, by) = tab.view.window_to_buffer_coords(
            gtk::TextWindowType::Widget,
            crate::app::px(x),
            crate::app::px(y),
        );
        if let Some(iter) = tab.view.iter_at_location(bx, by) {
            tab.buffer.place_cursor(&iter);
            launch::follow_at(&app, &iter);
        }
    });
    tab.view.add_controller(click);
    let weak = app.this.clone();
    let tab_weak = Rc::downgrade(tab);
    // Not end-user-action: vim keeps one user action open for a whole insert
    // session (one undo step), so that signal only fires on Esc.
    tab.buffer.connect_changed(move |_| {
        let (Some(app), Some(tab)) = (weak.upgrade(), tab_weak.upgrade()) else {
            return;
        };
        completion::typed(&app, &tab);
    });
}

fn on_key(
    app: &Rc<App>,
    tab: &Rc<Tab>,
    keyval: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> glib::Propagation {
    use gtk::gdk::{Key, ModifierType as M};
    if completion::key(app, tab, keyval, state) {
        return glib::Propagation::Stop;
    }
    // Ctrl+V with an image, in insert mode or with vim off (G2); otherwise
    // the key is vim's or GTK's own paste.
    if vim::inserting(tab) && tab.view.is_editable() && crate::attach::wants_ctrl_v(keyval, state) {
        crate::attach::paste_image(app);
        return glib::Propagation::Stop;
    }
    if let Some(entry) = registry::chord(keyval, state) {
        (entry.run)(app, "");
        return glib::Propagation::Stop;
    }
    if state.intersects(M::CONTROL_MASK | M::ALT_MASK)
        || !tab.view.is_editable()
        || !vim::inserting(tab)
    {
        return glib::Propagation::Proceed;
    }
    let handled = match keyval {
        Key::Return | Key::KP_Enter if !state.contains(M::SHIFT_MASK) => continue_list(tab),
        Key::Tab => indent(tab, false),
        Key::ISO_Left_Tab => indent(tab, true),
        k => k
            .to_unicode()
            .is_some_and(|c| !c.is_control() && pair(app, tab, c)),
    };
    if handled {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

/// `Enter` on a list line: the next marker, or the empty marker removed.
fn continue_list(tab: &Tab) -> bool {
    let (line, start, col) = current_line(tab);
    match edit::continue_list(&line, col) {
        Some(e) => {
            apply(tab, &line, &start, &e);
            true
        }
        None => false,
    }
}

/// `Tab`/`Shift+Tab` on a list line, using the note's own indent unit.
fn indent(tab: &Tab, dedent: bool) -> bool {
    let (line, _, col) = current_line(tab);
    if edit::continue_list(&line, col).is_none() && !is_list(&line) {
        return false;
    }
    // indent_list needs the whole text for the note's indent convention.
    let text = tab.text();
    let cursor = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
    let offset = tab
        .buffer
        .text(&tab.buffer.start_iter(), &cursor, true)
        .len();
    match edit::indent_list(&text, offset, dedent) {
        Some(mut e) => {
            e.cursor = follow_text(&e, offset);
            apply(tab, &text, &tab.buffer.start_iter(), &e);
            true
        }
        None => false,
    }
}

/// Where a cursor that was at `offset` ends up when the text around it moves.
fn follow_text(e: &TextEdit, offset: usize) -> usize {
    if offset >= e.range.end {
        offset - e.range.end + e.range.start + e.replacement.len()
    } else {
        e.range.start + e.replacement.len()
    }
}

fn is_list(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") || {
        let n = t.bytes().take_while(u8::is_ascii_digit).count();
        n > 0 && t[n..].starts_with(". ")
    }
}

/// The closer of each opener `edit::auto_pair` knows.
fn closer(open: &str) -> &'static str {
    match open {
        "[[" => "]]",
        "**" => "**",
        "`" => "`",
        "(" => ")",
        "[" => "]",
        "\"" => "\"",
        _ => "",
    }
}

/// Auto-pair (`editor.auto_pair`): typing the opener inserts it and its
/// closer; typing a closer that is already next steps over it.
fn pair(app: &App, tab: &Tab, c: char) -> bool {
    let enabled = app.config.borrow().editor.auto_pair.clone();
    if enabled.is_empty() {
        return false;
    }
    let (line, start, col) = current_line(tab);
    if enabled.iter().any(|o| closer(o).contains(c)) && line[col..].starts_with(c) {
        let mut at = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
        at.forward_char();
        tab.buffer.place_cursor(&at);
        return true;
    }
    if !enabled.iter().any(|o| o.ends_with(c)) {
        return false;
    }
    let _ = start;
    let mut typed = String::with_capacity(line.len() + c.len_utf8());
    typed.push_str(&line[..col]);
    typed.push(c);
    typed.push_str(&line[col..]);
    let Some(mut e) = edit::auto_pair(&typed, col + c.len_utf8(), &enabled) else {
        return false;
    };
    // Insert the typed character together with its closer.
    e.range = col..col;
    e.replacement = format!("{c}{}", e.replacement);
    e.cursor = col + c.len_utf8();
    let (line, start, _) = current_line(tab);
    apply(tab, &line, &start, &e);
    true
}

/// `Ctrl+Enter`, `:check`.
pub fn toggle_task(app: &Rc<App>) {
    let tab = app.tabs.current();
    if !tab.view.is_editable() {
        return;
    }
    let (line, start, col) = current_line(&tab);
    let cursor = tab.buffer.iter_at_mark(&tab.buffer.get_insert()).offset();
    if let Some(e) = edit::toggle_task(&line, col) {
        apply(&tab, &line, &start, &e);
        // Toggling keeps the cursor where it was.
        tab.buffer.place_cursor(&tab.buffer.iter_at_offset(cursor));
    }
}

/// `Ctrl+Shift+Up` (-1, promote) and `Ctrl+Shift+Down` (1, demote).
pub fn cycle_heading(app: &Rc<App>, direction: i8) {
    let tab = app.tabs.current();
    if !tab.view.is_editable() {
        return;
    }
    let (line, start, col) = current_line(&tab);
    if let Some(mut e) = edit::cycle_heading(&line, col, direction) {
        e.cursor = follow_text(&e, col);
        apply(&tab, &line, &start, &e);
    }
}
