//! The action registry (P§13, §17): one table drives the window shortcuts,
//! the files-column keys, the `:` command router, the palette and `F1`.
use crate::{app::App, editor, files, gtk};
use gtk::prelude::*;
use std::rc::Rc;

/// Where an entry's keys are live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// GTK accelerators on the window, bubble phase (an entry eats them first).
    Window,
    /// Vim-notation sequences, only while the files column has the keyboard.
    Files,
}

pub type Handler = fn(&Rc<App>, &str);

pub struct Entry {
    pub name: &'static str,
    pub label: &'static str,
    /// Window: GTK accelerator syntax (`<Alt>o`). Files/Reading: vim notation (`gg`, `<C-d>`).
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

use Scope::{Files as F, Window as W};

pub static ENTRIES: &[Entry] = &[
    entry("save", "Save note", W, &["<Control>s"], &[], |app, _| {
        editor::save_current(app);
    }),
    entry("quit", "Quit", W, &["<Control>q"], &[], |app, _| {
        app.window.close();
    }),
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
        |app, _| app.right.set_visible(!app.right.is_visible()),
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
        |app, _| {
            crate::help::show(app);
        },
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
        "files-down",
        "Cursor down",
        F,
        &["j", "<Down>"],
        &[],
        |app, _| {
            app.files.step(app, 1, true);
        },
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
        |app, _| {
            app.files.jump(app, 0);
        },
    ),
    entry(
        "files-bottom",
        "Last entry",
        F,
        &["G", "<End>"],
        &[],
        |app, _| {
            app.files.jump(app, u32::MAX);
        },
    ),
    entry(
        "files-half-down",
        "Half a page down",
        F,
        &["<C-d>"],
        &[],
        |app, _| {
            app.files.page(app, 0.5);
        },
    ),
    entry(
        "files-half-up",
        "Half a page up",
        F,
        &["<C-u>"],
        &[],
        |app, _| {
            app.files.page(app, -0.5);
        },
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
        "Enter the folder or open the note",
        F,
        &["l", "<CR>", "o", "<Right>"],
        &[],
        |app, _| app.files.open(app),
    ),
    entry(
        "files-keys",
        "List every key and command",
        F,
        &["~"],
        &[],
        |app, _| {
            crate::help::show(app);
        },
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

#[cfg(debug_assertions)]
pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.name == name)
}

/// Run an entry by name (palette, debug D-Bus action).
#[cfg(debug_assertions)]
pub fn run(app: &Rc<App>, name: &str, arg: &str) -> bool {
    match find(name) {
        Some(e) => {
            (e.run)(app, arg);
            true
        }
        None => false,
    }
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
    if scope != Scope::Window {
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
                assert!(
                    keys.insert((format!("{:?}", e.scope), (*k).to_owned())),
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
