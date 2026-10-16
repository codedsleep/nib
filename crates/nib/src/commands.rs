//! The `:` command router (P§10, §17) and the commands that act on tabs.
//! Every name is a registry entry; `GtkSourceView`'s own vim commands
//! (`:s`, `:set`, `:12`, …) are passed back to it.
use crate::{app::App, dialogs, editor, gtk, registry};
use gtk::prelude::*;
use std::rc::Rc;

/// `execute-command` handler: true when nib ran the command.
pub fn execute(app: &Rc<App>, line: &str) -> bool {
    let line = line.trim().trim_start_matches(':').trim();
    if line.is_empty() {
        return true;
    }
    let (name, arg) = line.split_once(' ').unwrap_or((line, ""));
    if let Some(entry) = registry::by_command(name) {
        // Run once vim has finished with the command bar: `:wq` closes the
        // very view vim is dispatching for.
        let (run, arg, weak) = (entry.run, arg.trim().to_owned(), app.this.clone());
        glib::idle_add_local_full(glib::Priority::HIGH, move || {
            if let Some(app) = weak.upgrade() {
                run(&app, &arg);
            }
            glib::ControlFlow::Break
        });
        return true;
    }
    if !vim_builtin(line) {
        app.status.error(&format!("not an editor command: {line}"));
    }
    false
}

/// Commands `GtkSourceView`'s vim handles itself (`gtksourcevimcommand.c`, 5.20).
pub fn vim_builtin(line: &str) -> bool {
    let rest = line.trim_start_matches(|c: char| {
        c.is_ascii_digit() || matches!(c, ',' | '.' | '$' | '%' | '\'' | '<' | '>' | '+' | '-')
    });
    if rest.is_empty() || rest.starts_with(['/', '?', 's']) {
        return true;
    }
    let name = rest.split(' ').next().unwrap_or_default();
    [
        "colorscheme",
        "delete",
        "j",
        "join",
        "nohl",
        "redo",
        "set",
        "sort",
        "u",
        "undo",
        "y",
        "yank",
    ]
    .contains(&name)
}

/// `:w [NAME]`, `Ctrl+S`. A name gives an untitled tab its note.
pub fn write(app: &Rc<App>, arg: &str) {
    let tab = app.tabs.current();
    if arg.is_empty() {
        if tab.path.borrow().is_none() {
            name_untitled(app);
        } else {
            editor::save(app, &tab);
        }
        return;
    }
    if tab.path.borrow().is_some() {
        app.status
            .error("this tab already has a note; :rename renames it");
        return;
    }
    editor::save_as(app, &tab, arg);
}

/// Untitled tabs ask for a name on `:w` (9.1).
pub fn name_untitled(app: &Rc<App>) {
    dialogs::prompt(app, "Name for the new note", "", |app, name| {
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            let tab = app.tabs.current();
            editor::save_as(app, &tab, name.trim());
        }
    });
}

/// `:wq`, `:x`: save, then close the tab.
pub fn write_quit(app: &Rc<App>, _: &str) {
    let tab = app.tabs.current();
    if tab.path.borrow().is_none() && tab.buffer.is_modified() {
        name_untitled(app);
        return;
    }
    if !tab.buffer.is_modified() || editor::save(app, &tab) {
        app.tabs.close(app, &tab);
    }
}

/// `:q`, `Alt+W`: close the tab.
pub fn quit(app: &Rc<App>, _: &str) {
    let tab = app.tabs.current();
    close_tab(app, &tab);
}

/// Close a tab. With autosave on, its edits are saved first; with autosave
/// off a modified tab asks.
pub fn close_tab(app: &Rc<App>, tab: &Rc<crate::editor::Tab>) {
    if !tab.buffer.is_modified() {
        app.tabs.close(app, tab);
        return;
    }
    if app.config.borrow().editor.autosave && tab.path.borrow().is_some() {
        if editor::save(app, tab) {
            app.tabs.close(app, tab);
        }
        return;
    }
    let question = format!(
        "{} has unsaved changes. Close it anyway?",
        tab.display_path(app)
    );
    let tab = tab.clone();
    dialogs::ask(app, &question, move |app, yes| {
        if yes {
            app.tabs.close(app, &tab);
        }
    });
}

/// `:tab N|new|close|next|prev`.
pub fn tab(app: &Rc<App>, arg: &str) {
    match arg {
        "" | "new" => {
            app.tabs.new_tab();
            app.tabs.focus_current();
        }
        "close" => quit(app, ""),
        "next" => app.tabs.cycle(1),
        "prev" | "previous" => app.tabs.cycle(-1),
        n => match n.parse::<usize>() {
            Ok(n) if (1..=app.tabs.len()).contains(&n) => {
                app.tabs.select(n - 1);
                app.tabs.focus_current();
            }
            _ => app
                .status
                .error(&format!("no tab {n}: :tab N|new|close|next|prev")),
        },
    }
}

/// `:{N}` for vim-off users (vim handles `:N` itself): go to a line.
pub fn goto_line(app: &Rc<App>, arg: &str) {
    let go = |app: &Rc<App>, text: &str| match text.trim().parse::<u32>() {
        Ok(n) => {
            let tab = app.tabs.current();
            tab.goto_line(n);
            tab.view.grab_focus();
        }
        Err(_) => app.status.error("a line number"),
    };
    if arg.is_empty() {
        dialogs::prompt(app, "Go to line", "", move |app, text| {
            if let Some(text) = text {
                go(app, &text);
            }
        });
    } else {
        go(app, arg);
    }
}

/// `:reindex`: parse the whole vault again.
pub fn reindex(app: &Rc<App>, _: &str) {
    app.index_ready.set(false);
    app.vault.start_index();
    app.status.set_index("indexing…");
}

/// `:q!`: close the tab, dropping unsaved edits.
pub fn quit_force(app: &Rc<App>, _: &str) {
    let tab = app.tabs.current();
    app.tabs.close(app, &tab);
}

/// `:qa`, `Ctrl+Q`: close the window, asking once (listing the notes) when
/// autosave is off and anything is modified.
pub fn quit_all(app: &Rc<App>, _: &str) {
    let autosave = app.config.borrow().editor.autosave;
    let modified: Vec<String> = app
        .tabs
        .all()
        .iter()
        .filter(|t| t.buffer.is_modified() && (!autosave || t.path.borrow().is_none()))
        .map(|t| t.display_path(app))
        .collect();
    if modified.is_empty() {
        app.window.close();
        return;
    }
    let question = format!("Unsaved changes in {}. Quit anyway?", modified.join(", "));
    dialogs::ask(app, &question, |app, yes| {
        if yes {
            app.discard_on_close.set(true);
            app.window.close();
        }
    });
}

/// `:e NOTE` opens (or creates, like a followed link) a note; `:e` alone
/// reloads an unmodified tab.
pub fn edit(app: &Rc<App>, arg: &str) {
    if arg.is_empty() {
        let tab = app.tabs.current();
        if tab.buffer.is_modified() {
            app.status
                .error("unsaved changes — :e! reloads from disk and drops them");
        } else {
            reload(app, "");
        }
        return;
    }
    crate::launch::open_name(app, arg);
}

/// `:e!`: reload the current note from disk, dropping edits and undo.
pub fn reload(app: &Rc<App>, _: &str) {
    let tab = app.tabs.current();
    let Some(path) = tab.path.borrow().clone() else {
        return;
    };
    match app.vault.read_note(&path) {
        Ok(note) => {
            let line = u32::try_from(tab.cursor_line()).unwrap_or(1);
            tab.load(app, &path, note, line);
            app.status
                .notice(&format!("reloaded {}", app.relative(&path)));
        }
        Err(e) => app.status.error(&format!("cannot reload: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::vim_builtin;
    #[test]
    fn builtins() {
        for b in [
            "12", "%s/a/b/g", "s/a/b/", "set ts=2", "1,5sort", "/find", "nohl", "u",
        ] {
            assert!(vim_builtin(b), "{b}");
        }
        for n in ["w", "wq", "frob", "e note", "x"] {
            assert!(!vim_builtin(n), "{n}");
        }
    }
}
