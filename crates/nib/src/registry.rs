//! The action registry (P§13, §17): one table drives the window shortcuts,
//! the files-column keys, the `:` command router, the palette and `F1`.
use crate::{app::App, commands, editor, fileops, files, finder, gtk, launch, panel};
use gtk::prelude::*;
use nib_core::settings::Sort;
use std::rc::Rc;

/// Where an entry's keys are live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// GTK accelerators on the window, bubble phase (an entry eats them first).
    Window,
    /// Accelerator chords the editor's own controller runs before vim sees them.
    Editor,
    /// Vim-notation sequences, only while the files column has the keyboard.
    Files,
    /// Only a `:` command or a palette row.
    Command,
}

pub type Handler = fn(&Rc<App>, &str);

pub struct Entry {
    pub name: &'static str,
    pub label: &'static str,
    /// Window/Editor: GTK accelerator syntax (`<Alt>o`). Files: vim notation (`gg`, `<C-d>`).
    pub keys: &'static [&'static str],
    pub scope: Scope,
    /// `:` command names; the first is shown in the palette.
    pub commands: &'static [&'static str],
    pub run: Handler,
}

const fn entry(
    name: &'static str,
    label: &'static str,
    scope: Scope,
    keys: &'static [&'static str],
    commands: &'static [&'static str],
    run: Handler,
) -> Entry {
    Entry {
        name,
        label,
        keys,
        scope,
        commands,
        run,
    }
}

use Scope::{Command as C, Editor as E, Files as F, Window as W};

pub static ENTRIES: &[Entry] = &[
    // ---- notes and tabs ----
    entry(
        "save",
        "Save note",
        W,
        &["<Control>s"],
        &["w", "w!", "write"],
        commands::write,
    ),
    entry(
        "write-quit",
        "Save and close the tab",
        C,
        &[],
        &["wq", "x"],
        commands::write_quit,
    ),
    entry(
        "close-tab",
        "Close the tab",
        C,
        &[],
        &["q", "quit"],
        commands::quit,
    ),
    entry(
        "close-tab-force",
        "Close the tab, dropping changes",
        C,
        &[],
        &["q!"],
        commands::quit_force,
    ),
    entry(
        "quit",
        "Quit",
        W,
        &["<Control>q"],
        &["qa", "qa!", "qall"],
        commands::quit_all,
    ),
    entry(
        "edit-note",
        "Open a note by name",
        C,
        &[],
        &["e"],
        commands::edit,
    ),
    entry(
        "reload-note",
        "Reload the note from disk, dropping changes",
        C,
        &[],
        &["e!"],
        commands::reload,
    ),
    entry(
        "rename",
        "Rename (rewriting links)",
        C,
        &[],
        &["rename"],
        fileops::rename_command,
    ),
    entry(
        "trash",
        "Move to the trash",
        C,
        &[],
        &["trash"],
        fileops::trash_command,
    ),
    // ---- finder ----
    entry(
        "switcher",
        "Open a note (quick switcher)",
        W,
        &["<Alt>o"],
        &["o", "open"],
        |app, arg| {
            finder::open(app, finder::Switcher, arg);
        },
    ),
    entry(
        "palette",
        "Command palette",
        W,
        &["<Alt>p"],
        &[],
        |app, _| {
            finder::open(app, finder::Palette, "");
        },
    ),
    entry(
        "search",
        "Search the vault",
        W,
        &["<Alt>f"],
        &["find"],
        |app, arg| {
            finder::open(app, finder::Search, arg);
        },
    ),
    entry("tag", "Notes by tag", C, &[], &["tag"], |app, arg| {
        if arg.is_empty() {
            finder::open(app, finder::Tags, "");
        } else {
            finder::open(app, finder::TagNotes(arg.to_owned()), "");
        }
    }),
    // ---- editor chords ----
    entry(
        "follow",
        "Follow the link under the cursor",
        E,
        &["<Alt>Return"],
        &["follow"],
        |app, _| {
            launch::follow_cursor(app);
        },
    ),
    entry(
        "check",
        "Toggle the task checkbox",
        E,
        &["<Control>Return"],
        &["check"],
        |app, _| {
            editor::behaviours::toggle_task(app);
        },
    ),
    entry(
        "complete",
        "Complete the link or tag at the cursor",
        E,
        &["<Control>space"],
        &[],
        |app, _| {
            editor::completion::force(app);
        },
    ),
    entry(
        "heading-up",
        "Promote the heading",
        E,
        &["<Control><Shift>Up"],
        &[],
        |app, _| {
            editor::behaviours::cycle_heading(app, -1);
        },
    ),
    entry(
        "heading-down",
        "Demote the heading",
        E,
        &["<Control><Shift>Down"],
        &[],
        |app, _| {
            editor::behaviours::cycle_heading(app, 1);
        },
    ),
    // ---- window ----
    entry(
        "read",
        "Reading view (toggle)",
        W,
        &["<Alt>e"],
        &["read"],
        |app, _| {
            crate::reading::toggle(app);
        },
    ),
    entry(
        "edit-source",
        "Back to the source (:edit NOTE opens one)",
        C,
        &[],
        &["edit"],
        |app, arg| {
            if arg.is_empty() {
                let tab = app.tabs.current();
                crate::reading::set_mode(app, &tab, crate::reading::Mode::Source);
            } else {
                commands::edit(app, arg);
            }
        },
    ),
    entry(
        "split",
        "Source and reading side by side",
        W,
        &["<Alt><Shift>e"],
        &["split"],
        |app, _| {
            crate::reading::toggle_split(app);
        },
    ),
    entry(
        "toggle-files",
        "Show or hide the files column",
        W,
        &["F9"],
        &["files"],
        |app, _| files::toggle(app),
    ),
    entry(
        "toggle-panel",
        "Show or hide the right panel",
        W,
        &["F8"],
        &[],
        |app, _| panel::toggle(app),
    ),
    entry(
        "parent-column",
        "Show or hide the parent column",
        W,
        &["F7"],
        &[],
        |app, _| {
            app.files.toggle_parent(app);
        },
    ),
    entry(
        "backlinks",
        "Focus the backlinks",
        W,
        &["<Alt>b"],
        &["backlinks"],
        |app, _| {
            panel::focus(app, panel::Page::Backlinks);
        },
    ),
    entry(
        "outline",
        "Focus the outline",
        C,
        &[],
        &["outline"],
        |app, _| {
            panel::focus(app, panel::Page::Outline);
        },
    ),
    entry("fullscreen", "Fullscreen", W, &["F11"], &[], |app, _| {
        if app.window.is_fullscreen() {
            app.window.unfullscreen();
        } else {
            app.window.fullscreen();
        }
    }),
    entry(
        "reload-folder",
        "Reload the folder listing from disk",
        W,
        &["F5"],
        &[],
        |app, _| app.files.reload(app),
    ),
    entry(
        "keys",
        "List every key and command",
        W,
        &["F1"],
        &["help"],
        |app, _| crate::help::show(app),
    ),
    entry(
        "focus-files",
        "Focus the files column",
        W,
        &["<Alt>1"],
        &[],
        |app, _| files::focus(app),
    ),
    entry(
        "focus-editor",
        "Focus the editor",
        W,
        &["<Alt>2"],
        &[],
        |app, _| editor::focus(app),
    ),
    entry(
        "focus-panel",
        "Focus the right panel",
        W,
        &["<Alt>3"],
        &[],
        |app, _| {
            panel::focus(app, panel::Page::Current);
        },
    ),
    entry(
        "version",
        "Show nib's version",
        C,
        &[],
        &["version"],
        |app, _| {
            app.status
                .notice(concat!("nib ", env!("CARGO_PKG_VERSION")));
        },
    ),
    // ---- files column ----
    entry(
        "files-down",
        "Cursor down",
        F,
        &["j", "<Down>"],
        &[],
        |app, _| app.files.step(app, 1, true),
    ),
    entry("files-up", "Cursor up", F, &["k", "<Up>"], &[], |app, _| {
        app.files.step(app, -1, true);
    }),
    entry(
        "files-top",
        "First entry",
        F,
        &["gg", "<Home>"],
        &[],
        |app, _| app.files.jump(app, 0),
    ),
    entry(
        "files-bottom",
        "Last entry",
        F,
        &["G", "<End>"],
        &[],
        |app, _| app.files.jump(app, u32::MAX),
    ),
    entry(
        "files-half-down",
        "Half a page down",
        F,
        &["<C-d>"],
        &[],
        |app, _| app.files.page(app, 0.5),
    ),
    entry(
        "files-half-up",
        "Half a page up",
        F,
        &["<C-u>"],
        &[],
        |app, _| app.files.page(app, -0.5),
    ),
    entry(
        "files-page-down",
        "A page down",
        F,
        &["<C-f>", "<PageDown>"],
        &[],
        |app, _| app.files.page(app, 1.0),
    ),
    entry(
        "files-page-up",
        "A page up",
        F,
        &["<C-b>", "<PageUp>"],
        &[],
        |app, _| app.files.page(app, -1.0),
    ),
    entry(
        "files-parent",
        "Parent folder",
        F,
        &["h", "<Left>", "<BS>"],
        &[],
        |app, _| app.files.parent(app),
    ),
    entry(
        "files-open",
        "Enter the folder or open the file",
        F,
        &["l", "<CR>", "o", "<Right>"],
        &[],
        |app, _| {
            app.files.open(app);
        },
    ),
    entry(
        "files-open-tab",
        "Open in a new tab",
        F,
        &["O"],
        &[],
        |app, _| app.files.open_new_tab(app),
    ),
    entry("files-back", "Previous folder", F, &["H"], &[], |app, _| {
        app.files.history(app, true);
    }),
    entry("files-forward", "Next folder", F, &["L"], &[], |app, _| {
        app.files.history(app, false);
    }),
    entry(
        "files-mark",
        "Select or unselect, then step down",
        F,
        &["<Space>"],
        &[],
        |app, _| {
            app.files.toggle_mark(app);
        },
    ),
    entry(
        "files-visual",
        "Visual selection mode",
        F,
        &["v"],
        &[],
        |app, _| app.files.visual_mode(app),
    ),
    entry(
        "files-select-all",
        "Select everything",
        F,
        &["<C-a>"],
        &[],
        |app, _| app.files.mark_all(),
    ),
    entry(
        "files-invert",
        "Invert the selection",
        F,
        &["<C-r>"],
        &[],
        |app, _| app.files.invert_marks(),
    ),
    entry(
        "files-clear",
        "Clear the selection and the filter",
        F,
        &["<Esc>"],
        &[],
        |app, _| {
            app.files.escape(app);
        },
    ),
    entry(
        "files-yank",
        "Yank (copy) to the clipboard",
        F,
        &["y"],
        &[],
        |app, _| fileops::yank(app, false),
    ),
    entry(
        "files-cut",
        "Cut to the clipboard",
        F,
        &["x"],
        &[],
        |app, _| fileops::yank(app, true),
    ),
    entry("files-paste", "Paste here", F, &["p"], &[], |app, _| {
        fileops::paste(app, false);
    }),
    entry(
        "files-paste-over",
        "Paste here, replacing existing names",
        F,
        &["P"],
        &[],
        |app, _| {
            fileops::paste(app, true);
        },
    ),
    entry(
        "files-unyank",
        "Forget the yank",
        F,
        &["Y"],
        &[],
        |app, _| fileops::unyank(app),
    ),
    entry(
        "files-trash",
        "Move to the trash",
        F,
        &["d"],
        &[],
        |app, _| fileops::trash(app, ""),
    ),
    entry(
        "files-create",
        "Create a note (name/ for a folder)",
        F,
        &["a"],
        &[],
        |app, _| fileops::create(app),
    ),
    entry(
        "files-rename",
        "Rename (rewriting links)",
        F,
        &["r"],
        &[],
        |app, _| fileops::rename(app, ""),
    ),
    entry(
        "files-find",
        "Find in the list",
        F,
        &["/"],
        &[],
        |app, _| app.files.open_prompt(Some(false)),
    ),
    entry(
        "files-find-back",
        "Find backwards",
        F,
        &["?"],
        &[],
        |app, _| app.files.open_prompt(Some(true)),
    ),
    entry("files-next", "Next match", F, &["n"], &[], |app, _| {
        app.files.find_next(false, false);
    }),
    entry(
        "files-previous",
        "Previous match",
        F,
        &["N"],
        &[],
        |app, _| app.files.find_next(true, false),
    ),
    entry(
        "files-filter",
        "Filter the list",
        F,
        &["f"],
        &[],
        |app, _| app.files.open_prompt(None),
    ),
    entry(
        "files-finder",
        "Fuzzy-find below this folder",
        F,
        &["z"],
        &[],
        |app, _| {
            finder::open(app, finder::Walk(app.files.folder()), "");
        },
    ),
    entry(
        "files-hidden",
        "Show or hide hidden files",
        F,
        &["."],
        &[],
        |app, _| app.files.toggle_hidden(app),
    ),
    entry(
        "files-sort-name",
        "Sort by name",
        F,
        &[",n"],
        &[],
        |app, _| app.files.sort_by(app, Sort::Name),
    ),
    entry(
        "files-sort-modified",
        "Sort by modified time, showing ages",
        F,
        &[",m"],
        &[],
        |app, _| {
            app.files.sort_by(app, Sort::Modified);
        },
    ),
    entry(
        "files-sort-reverse",
        "Reverse the order",
        F,
        &[",r"],
        &[],
        |app, _| app.files.reverse(app),
    ),
    entry(
        "files-copy-path",
        "Copy the absolute path",
        F,
        &["cp"],
        &[],
        |app, _| fileops::copy_text(app, 'p'),
    ),
    entry(
        "files-copy-name",
        "Copy the name",
        F,
        &["cn"],
        &[],
        |app, _| fileops::copy_text(app, 'n'),
    ),
    entry(
        "files-copy-link",
        "Copy a [[wikilink]]",
        F,
        &["cl"],
        &[],
        |app, _| fileops::copy_text(app, 'l'),
    ),
    entry(
        "files-copy-relative",
        "Copy the vault-relative path",
        F,
        &["cr"],
        &[],
        |app, _| {
            fileops::copy_text(app, 'r');
        },
    ),
    entry(
        "files-go-root",
        "Go to the vault root",
        F,
        &["gh"],
        &[],
        |app, _| app.files.go_to(app, ""),
    ),
    entry(
        "files-go-daily",
        "Go to the daily folder",
        F,
        &["gd"],
        &[],
        |app, _| {
            let rel = app.config.borrow().vault.daily_folder.clone();
            app.files.go_to(app, &rel);
        },
    ),
    entry(
        "files-go-attachments",
        "Go to the attachments folder",
        F,
        &["ga"],
        &[],
        |app, _| {
            let rel = app.config.borrow().vault.attachments.clone();
            app.files.go_to(app, &rel);
        },
    ),
    entry(
        "files-go-templates",
        "Go to the templates folder",
        F,
        &["gt"],
        &[],
        |app, _| {
            let rel = app.config.borrow().vault.templates.clone();
            app.files.go_to(app, &rel);
        },
    ),
    entry(
        "files-go-recent",
        "Recent notes",
        F,
        &["gr"],
        &[],
        |app, _| {
            finder::open(app, finder::Switcher, "");
        },
    ),
    entry(
        "files-keys",
        "List every key and command",
        F,
        &["~"],
        &[],
        |app, _| crate::help::show(app),
    ),
    entry(
        "files-to-editor",
        "Focus the editor",
        F,
        &["<Tab>"],
        &[],
        |app, _| editor::focus(app),
    ),
];

pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.name == name)
}

/// Run an entry by name (palette, debug D-Bus action).
pub fn run(app: &Rc<App>, name: &str, arg: &str) -> bool {
    match find(name) {
        Some(e) => {
            (e.run)(app, arg);
            true
        }
        None => false,
    }
}

pub fn by_command(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.commands.contains(&name))
}

thread_local! {
    /// Editor and window chords parsed once (GTK must be initialised to parse).
    static CHORDS: Vec<(gtk::gdk::Key, gtk::gdk::ModifierType, &'static Entry)> = ENTRIES
        .iter()
        .filter(|e| matches!(e.scope, Scope::Editor | Scope::Window))
        .flat_map(|e| e.keys.iter().map(move |k| (k, e)))
        .filter_map(|(k, e)| gtk::accelerator_parse(*k).map(|(key, mods)| (key.to_lower(), mods, e)))
        .collect();
}

/// The editor or window chord for a key press, if any. The editor runs
/// window chords itself: vim would otherwise eat `Alt+B` as `b`.
pub fn chord(keyval: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> Option<&'static Entry> {
    use gtk::gdk::ModifierType as M;
    let mods = state & (M::CONTROL_MASK | M::SHIFT_MASK | M::ALT_MASK);
    let key = keyval.to_lower();
    CHORDS.with(|keys| {
        keys.iter()
            .find(|(k, m, _)| *k == key && *m == mods)
            .map(|(_, _, e)| *e)
    })
}

/// How a typed files-column sequence relates to the table.
pub enum Lookup {
    Exact(&'static Entry),
    Prefix,
    None,
}

pub fn lookup(scope: Scope, sequence: &str) -> Lookup {
    let mut prefix = false;
    for e in ENTRIES.iter().filter(|e| e.scope == scope) {
        for k in e.keys {
            if *k == sequence {
                return Lookup::Exact(e);
            }
            prefix |= k.len() > sequence.len() && k.starts_with(sequence) && !k.starts_with('<');
        }
    }
    if prefix { Lookup::Prefix } else { Lookup::None }
}

/// Window accelerators, bubble phase, so a focused entry or vim gets them first.
pub fn install(app: &Rc<App>) {
    let controller = gtk::ShortcutController::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Bubble);
    for e in ENTRIES.iter().filter(|e| e.scope == Scope::Window) {
        for key in e.keys {
            let Some(trigger) = gtk::ShortcutTrigger::parse_string(key) else {
                log::warn!("bad accelerator {key} for {}", e.name);
                continue;
            };
            let weak = app.this.clone();
            let run = e.run;
            let action = gtk::CallbackAction::new(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    run(&app, "");
                }
                glib::Propagation::Stop
            });
            controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
        }
    }
    app.window.add_controller(controller);
    #[cfg(debug_assertions)]
    {
        // Lets agent checks run any entry over D-Bus: `win.run "name arg"`.
        let action = gio::SimpleAction::new("run", Some(glib::VariantTy::STRING));
        let weak = app.this.clone();
        action.connect_activate(move |_, param| {
            let (Some(app), Some(text)) = (weak.upgrade(), param.and_then(|p| p.str())) else {
                return;
            };
            let (name, arg) = text.split_once(' ').unwrap_or((text, ""));
            if !run(&app, name, arg) {
                app.status.error(&format!("no such action: {name}"));
            }
        });
        app.window.add_action(&action);
    }
}

/// `<Alt>o` → `Alt+O`, `<Control><Shift>Up` → `Ctrl+Shift+Up`; vim notation is shown as is.
pub fn display_key(scope: Scope, key: &str) -> String {
    if !matches!(scope, Scope::Window | Scope::Editor) {
        return key.to_owned();
    }
    let mut out = String::new();
    let mut rest = key;
    while let Some(stripped) = rest.strip_prefix('<') {
        let Some((m, after)) = stripped.split_once('>') else {
            break;
        };
        out.push_str(match m {
            "Control" | "Primary" => "Ctrl+",
            "Shift" => "Shift+",
            "Alt" => "Alt+",
            other => other,
        });
        rest = after;
    }
    let name = match rest {
        "Return" => "Enter",
        "bracketleft" => "[",
        "bracketright" => "]",
        "space" => "Space",
        "Escape" => "Esc",
        r if r.len() == 1 => {
            out.push_str(&r.to_uppercase());
            return out;
        }
        r => r,
    };
    out.push_str(name);
    out
}

/// The vim-notation name of a key press, for the files column and reading view.
pub fn key_name(keyval: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> Option<String> {
    use gtk::gdk::{Key, ModifierType as M};
    let named = match keyval {
        Key::Return | Key::KP_Enter => Some("CR"),
        Key::Escape => Some("Esc"),
        Key::Tab => Some("Tab"),
        Key::ISO_Left_Tab => Some("S-Tab"),
        Key::space => Some("Space"),
        Key::BackSpace => Some("BS"),
        Key::Up => Some("Up"),
        Key::Down => Some("Down"),
        Key::Left => Some("Left"),
        Key::Right => Some("Right"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::Page_Up => Some("PageUp"),
        Key::Page_Down => Some("PageDown"),
        Key::Delete => Some("Del"),
        _ => None,
    };
    let ctrl = state.contains(M::CONTROL_MASK);
    let alt = state.contains(M::ALT_MASK);
    if let Some(n) = named {
        let prefix = match (ctrl, alt) {
            (true, _) => "C-",
            (_, true) => "A-",
            _ => "",
        };
        return Some(format!("<{prefix}{n}>"));
    }
    if let Some(name) = keyval
        .name()
        .filter(|n| n.starts_with('F') && n.len() <= 3 && n[1..].parse::<u8>().is_ok())
    {
        return Some(format!("<{name}>"));
    }
    let c = keyval.to_unicode().filter(|c| !c.is_control())?;
    if ctrl {
        return Some(format!("<C-{}>", c.to_ascii_lowercase()));
    }
    if alt {
        return Some(format!("<A-{c}>"));
    }
    Some(c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn every_entry_is_labelled_and_keys_are_unique_per_scope() {
        let mut names = HashSet::new();
        let mut keys = HashSet::new();
        let mut commands = HashSet::new();
        for e in ENTRIES {
            assert!(!e.label.is_empty(), "{} has no label", e.name);
            assert!(names.insert(e.name), "duplicate name {}", e.name);
            for k in e.keys {
                // Editor chords and window accelerators share one namespace.
                let scope = if e.scope == Scope::Editor {
                    Scope::Window
                } else {
                    e.scope
                };
                assert!(
                    keys.insert((format!("{scope:?}"), (*k).to_owned())),
                    "{k} is bound twice in {:?}",
                    e.scope
                );
            }
            for c in e.commands {
                assert!(commands.insert(*c), "duplicate command :{c}");
            }
        }
    }
    #[test]
    fn files_sequences_are_never_ambiguous() {
        for e in ENTRIES.iter().filter(|e| e.scope == Scope::Files) {
            for k in e.keys {
                for other in ENTRIES.iter().filter(|o| o.scope == Scope::Files) {
                    for o in other.keys {
                        assert!(
                            k == o || !o.starts_with(*k) || k.starts_with('<'),
                            "{k} is a prefix of {o}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn display() {
        assert_eq!(display_key(Scope::Window, "<Alt>o"), "Alt+O");
        assert_eq!(
            display_key(Scope::Window, "<Control><Shift>Up"),
            "Ctrl+Shift+Up"
        );
        assert_eq!(display_key(Scope::Window, "<Alt>bracketleft"), "Alt+[");
        assert_eq!(display_key(Scope::Files, "gg"), "gg");
    }
}
