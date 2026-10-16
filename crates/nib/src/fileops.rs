//! File operations from the files column (P§8, D9): create, link-aware
//! rename, trash, yank/cut/paste. Each runs on the vault's worker; this
//! module asks the questions, shows progress and acts on the result.
use crate::{app::App, clipboard, dialogs, editor, gtk};
use gtk::prelude::*;
use nib_core::{Edit, LinkTarget, NoteId, settings::Rewrite};
use nib_vault::{ConflictAnswer, OpEvent, OpId, OpSummary, ops::FileOp};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    rc::Rc,
};

/// What to do when an operation finishes.
pub enum Pending {
    Rename {
        from: PathBuf,
        to: PathBuf,
        plan: Vec<Edit>,
    },
    Edits,
    Trash {
        names: Vec<String>,
        linked: Vec<(String, usize)>,
    },
    Paste {
        cut: bool,
        overwrite: bool,
    },
    CreateNote {
        path: PathBuf,
    },
    CreateFolder {
        path: PathBuf,
    },
}

fn verb(p: &Pending) -> &'static str {
    match p {
        Pending::Rename { .. } => "renaming",
        Pending::Edits => "updating links",
        Pending::Trash { .. } => "trashing",
        Pending::Paste { cut: true, .. } => "moving",
        Pending::Paste { .. } => "copying",
        Pending::CreateNote { .. } | Pending::CreateFolder { .. } => "creating",
    }
}

fn start(app: &Rc<App>, op: FileOp, pending: Pending) -> OpId {
    let id = app.vault.op(op);
    app.ops.borrow_mut().insert(id, pending);
    id
}

pub fn on_event(app: &Rc<App>, event: OpEvent) {
    match event {
        OpEvent::Progress { id, done, total } => {
            if let Some(p) = app.ops.borrow().get(&id) {
                app.status.notice(&format!("{} {done}/{total}…", verb(p)));
            }
        }
        OpEvent::Conflict { id, path } => conflict(app, id, &path),
        OpEvent::Done { id, summary } => {
            let pending = app.ops.borrow_mut().remove(&id);
            if let Some(p) = pending {
                done(app, p, &summary);
            }
        }
        OpEvent::Failed { id, message } => {
            app.ops.borrow_mut().remove(&id);
            app.status.error(&message);
        }
    }
}

/// Asked once per operation; the answer applies to its later conflicts.
fn conflict(app: &Rc<App>, id: OpId, path: &Path) {
    let overwrite = matches!(
        app.ops.borrow().get(&id),
        Some(Pending::Paste {
            overwrite: true,
            ..
        })
    );
    if overwrite {
        app.vault.answer(id, ConflictAnswer::Replace);
        return;
    }
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let question = format!("{name} already exists here.");
    dialogs::choose(
        app,
        &question,
        &[
            ('s', "skip"),
            ('r', "replace (old one to trash)"),
            ('n', "keep both"),
        ],
        move |app, answer| {
            let answer = match answer {
                Some('r') => ConflictAnswer::Replace,
                Some('n') => ConflictAnswer::Rename,
                _ => ConflictAnswer::Skip,
            };
            app.vault.answer(id, answer);
        },
    );
}

fn done(app: &Rc<App>, pending: Pending, summary: &OpSummary) {
    match pending {
        Pending::Rename { from, to, plan } => {
            app.tabs.follow_rename(app, &from, &to);
            let name = to
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            app.files.want_cursor(&name);
            if plan.is_empty() {
                app.status.notice(&format!("renamed to {name}"));
            } else {
                start(app, FileOp::ApplyEdits(plan), Pending::Edits);
            }
        }
        Pending::Edits => {
            patch_buffers(app, &summary.edits);
            let notes = summary
                .edits
                .iter()
                .map(|e| &e.note)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            app.status.notice(&format!(
                "renamed; updated {} in {}",
                plural(summary.links, "link"),
                plural(notes, "note")
            ));
        }
        Pending::Trash { names, linked } => {
            let mut message = match names.as_slice() {
                [one] => format!("trashed {one}"),
                _ => format!("trashed {}", plural(names.len(), "item")),
            };
            for (name, n) in linked {
                let _ = write!(
                    message,
                    " — {name} is still linked from {}",
                    plural(n, "note")
                );
            }
            app.status.notice(&message);
        }
        Pending::Paste { cut, .. } => {
            if cut {
                clipboard::clear_own();
                app.files.clear_cut();
            }
            app.status.notice(&format!(
                "{} {}",
                if cut { "moved" } else { "copied" },
                plural(summary.files, "item")
            ));
        }
        Pending::CreateNote { path } => {
            if let Some(name) = path.file_name() {
                app.files.want_cursor(&name.to_string_lossy());
            }
            app.open_note(&path, false, None);
            app.status
                .notice(&format!("created {}", app.relative(&path)));
        }
        Pending::CreateFolder { path } => {
            if let Some(name) = path.file_name() {
                app.files.want_cursor(&name.to_string_lossy());
            }
            app.status
                .notice(&format!("created {}/", app.relative(&path)));
        }
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Apply rename-plan edits to open buffers in place, as one user action per
/// buffer, so undo survives (and undoes the patch). Highest offset first.
pub fn patch_buffers(app: &Rc<App>, edits: &[Edit]) {
    let mut by_note: BTreeMap<&NoteId, Vec<&Edit>> = BTreeMap::new();
    for e in edits {
        by_note.entry(&e.note).or_default().push(e);
    }
    for (note, mut edits) in by_note {
        let path = app.vault.root().join(note.filename());
        let Some(tab) = app.tabs.find(&path) else {
            continue;
        };
        let text = tab.text();
        if nib_core::text::fingerprint(&text) != edits[0].source_hash {
            // The buffer is not what the plan saw; the file on disk is right.
            app.status.notice(&format!(
                "{} differs from disk — :e! loads the updated links",
                app.relative(&path)
            ));
            continue;
        }
        edits.sort_by_key(|e| std::cmp::Reverse(e.byte_range.start));
        let was_modified = tab.buffer.is_modified();
        let chars = |b: usize| i32::try_from(text[..b].chars().count()).unwrap_or(0);
        tab.buffer.begin_user_action();
        for e in edits {
            if text.get(e.byte_range.clone()) != Some(e.expected.as_str()) {
                continue;
            }
            let mut start = tab.buffer.iter_at_offset(chars(e.byte_range.start));
            let mut end = tab.buffer.iter_at_offset(chars(e.byte_range.end));
            tab.buffer.delete(&mut start, &mut end);
            let mut at = tab.buffer.iter_at_offset(chars(e.byte_range.start));
            tab.buffer.insert(&mut at, &e.replacement);
        }
        tab.buffer.end_user_action();
        if !was_modified {
            // The buffer now matches the bytes ApplyEdits wrote.
            tab.buffer.set_modified(false);
        }
    }
}

/// `a`: `name/` makes a folder; `name` makes `name.md` and opens it.
pub fn create(app: &Rc<App>) {
    let folder = app.files.folder();
    dialogs::prompt(
        app,
        "New note (name/ for a folder)",
        "",
        move |app, name| {
            let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else {
                return;
            };
            if let Some(dir) = name.strip_suffix('/') {
                if dir.is_empty() || dir.contains('/') {
                    app.status.error("a folder name, without slashes inside");
                    return;
                }
                let path = folder.join(dir);
                start(
                    app,
                    FileOp::CreateFolder { path: path.clone() },
                    Pending::CreateFolder { path },
                );
                return;
            }
            let path = Path::new(&name);
            if path
                .extension()
                .is_some_and(|e| !e.eq_ignore_ascii_case("md"))
            {
                app.status.error("nib creates notes and folders only");
                return;
            }
            let rel = folder
                .strip_prefix(app.vault.root())
                .unwrap_or(Path::new(""))
                .join(&name);
            let Some(id) = rel.to_str().and_then(|r| NoteId::new(r).ok()) else {
                app.status
                    .error(&format!("cannot make a note called {name}"));
                return;
            };
            let path = app.vault.root().join(id.filename());
            start(
                app,
                FileOp::CreateNote {
                    id,
                    initial: String::new(),
                },
                Pending::CreateNote { path },
            );
        },
    );
}

/// The note (or folder of notes) identity of a vault path, for rename plans.
fn plan_id(app: &App, path: &Path, directory: bool) -> Option<NoteId> {
    let rel = path.strip_prefix(app.vault.root()).ok()?.to_str()?;
    if directory || rel.to_ascii_lowercase().ends_with(".md") {
        NoteId::new(rel).ok()
    } else {
        None
    }
}

/// `r`, `:rename NAME`: rename the entry under the cursor, rewriting links
/// after asking (`links.rewrite_on_rename`).
pub fn rename(app: &Rc<App>, arg: &str) {
    let Some(entry) = app.files.cursor_entry() else {
        return;
    };
    // The plan must see what is on disk: save edits first (P§16).
    for tab in app.tabs.all() {
        editor::save_if_modified(app, &tab);
    }
    if arg.is_empty() {
        let initial = entry.name.clone();
        dialogs::prompt(app, "Rename to", &initial, move |app, name| {
            if let Some(name) = name {
                rename_to(app, &entry.path, entry.directory, name.trim());
            }
        });
    } else {
        rename_to(app, &entry.path, entry.directory, arg.trim());
    }
}

fn rename_to(app: &Rc<App>, from: &Path, directory: bool, name: &str) {
    if name.is_empty() || name.contains('/') {
        app.status.error("a new name, without slashes");
        return;
    }
    let old_name = from.file_name().map(|n| n.to_string_lossy().into_owned());
    let mut name = name.to_owned();
    // A note keeps being a note: `hub` → `hub.md`.
    if !directory
        && old_name
            .as_deref()
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".md"))
        && Path::new(&name).extension().is_none()
    {
        name.push_str(".md");
    }
    if old_name.as_deref() == Some(name.as_str()) {
        return;
    }
    let Some(parent) = from.parent() else { return };
    let to = parent.join(&name);
    let plan = match (plan_id(app, from, directory), plan_id(app, &to, directory)) {
        (Some(old), Some(new)) if app.index_ready.get() => {
            nib_core::rename_plan(&app.index.borrow(), &old, &new)
        }
        _ => vec![],
    };
    let policy = app.config.borrow().links.rewrite_on_rename;
    let from = from.to_owned();
    if plan.is_empty() || policy != Rewrite::Ask {
        let plan = if policy == Rewrite::Never {
            vec![]
        } else {
            plan
        };
        start(
            app,
            FileOp::Rename {
                from: from.clone(),
                to: to.clone(),
            },
            Pending::Rename { from, to, plan },
        );
        return;
    }
    let notes = plan
        .iter()
        .map(|e| &e.note)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let question = format!(
        "Update {} in {}?",
        plural(plan.len(), "link"),
        plural(notes, "note")
    );
    dialogs::ask(app, &question, move |app, yes| {
        let plan = if yes { plan } else { vec![] };
        start(
            app,
            FileOp::Rename {
                from: from.clone(),
                to: to.clone(),
            },
            Pending::Rename { from, to, plan },
        );
    });
}

/// `:rename NAME`: the files column's cursor row when the column has the
/// keyboard, else the current note.
pub fn rename_command(app: &Rc<App>, arg: &str) {
    if app.files.has_focus() {
        rename(app, arg);
        return;
    }
    let Some(path) = app.tabs.current().path.borrow().clone() else {
        app.status.error("this tab has no note to rename");
        return;
    };
    for tab in app.tabs.all() {
        editor::save_if_modified(app, &tab);
    }
    if arg.is_empty() {
        let initial = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        dialogs::prompt(app, "Rename to", &initial, move |app, name| {
            if let Some(name) = name {
                rename_to(app, &path, false, name.trim());
            }
        });
    } else {
        rename_to(app, &path, false, arg);
    }
}

/// `:trash`: the column's targets when it has the keyboard, else the current note.
pub fn trash_command(app: &Rc<App>, _: &str) {
    if app.files.has_focus() {
        trash(app, "");
        return;
    }
    let Some(path) = app.tabs.current().path.borrow().clone() else {
        return;
    };
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    start(
        app,
        FileOp::Trash { paths: vec![path] },
        Pending::Trash {
            names: vec![name],
            linked: vec![],
        },
    );
}

/// `d`, `:trash`: the selection (or the cursor row) to the XDG trash. A
/// still-linked attachment is trashed with a notice, not refused.
pub fn trash(app: &Rc<App>, _: &str) {
    let entries = app.files.targets();
    if entries.is_empty() {
        return;
    }
    let linked = {
        let index = app.index.borrow();
        entries
            .iter()
            .filter(|e| !e.directory && !e.name.to_ascii_lowercase().ends_with(".md"))
            .filter_map(|e| {
                let rel = e
                    .path
                    .strip_prefix(app.vault.root())
                    .ok()?
                    .to_str()?
                    .to_owned();
                let n = index
                    .notes
                    .values()
                    .filter(|m| {
                        m.links
                            .iter()
                            .any(|l| matches!(&l.target, LinkTarget::Attachment(p) if *p == rel))
                    })
                    .count();
                (n > 0).then(|| (e.name.clone(), n))
            })
            .collect()
    };
    let names = entries.iter().map(|e| e.name.clone()).collect();
    let paths = entries.into_iter().map(|e| e.path).collect();
    app.files.clear_marks(app);
    start(
        app,
        FileOp::Trash { paths },
        Pending::Trash { names, linked },
    );
}

/// `y` / `x`.
pub fn yank(app: &Rc<App>, cut: bool) {
    let entries = app.files.targets();
    if entries.is_empty() {
        return;
    }
    let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    match clipboard::set_files(&paths, cut) {
        Ok(()) => {
            app.files.mark_cut(if cut { &paths } else { &[] });
            app.files.clear_marks(app);
            app.status.notice(&format!(
                "{} {}",
                if cut { "cut" } else { "yanked" },
                plural(paths.len(), "item")
            ));
        }
        Err(e) => app.status.error(&format!("clipboard: {e}")),
    }
}

/// `p` / `P` (paste over: conflicts replace).
pub fn paste(app: &Rc<App>, overwrite: bool) {
    let destination = app.files.folder();
    let weak = app.this.clone();
    glib::spawn_future_local(async move {
        let files = clipboard::read_files().await;
        let Some(app) = weak.upgrade() else { return };
        let Some((sources, cut)) = files else {
            app.status.notice("no files on the clipboard");
            return;
        };
        let op = if cut {
            FileOp::Move {
                sources,
                destination,
            }
        } else {
            FileOp::Copy {
                sources,
                destination,
            }
        };
        start(&app, op, Pending::Paste { cut, overwrite });
    });
}

/// `Y`: forget the yank.
pub fn unyank(app: &Rc<App>) {
    clipboard::clear_own();
    app.files.clear_cut();
    app.status.notice("clipboard cleared");
}

/// `cp` `cn` `cr` `cl`: absolute path, name, vault-relative path, wikilink.
pub fn copy_text(app: &Rc<App>, what: char) {
    let entries = app.files.targets();
    if entries.is_empty() {
        return;
    }
    let lines: Vec<String> = entries
        .iter()
        .map(|e| match what {
            'p' => e.path.to_string_lossy().into_owned(),
            'n' => e.name.clone(),
            'r' => app.relative(&e.path),
            _ => wikilink(app, &e.path),
        })
        .collect();
    let text = lines.join(if what == 'l' { " " } else { "\n" });
    clipboard::set_text(&text);
    app.status.notice(&format!("copied {text}"));
}

fn wikilink(app: &App, path: &Path) -> String {
    let rel = app.relative(path);
    let fmt = app.config.borrow().links.format;
    let embed_or_link = match app.note_id(path) {
        Some(id) if app.index_ready.get() => nib_core::shortest_link(&app.index.borrow(), &id, fmt),
        Some(id) => id.to_string(),
        None => rel,
    };
    format!("[[{embed_or_link}]]")
}
