//! Daily notes, new notes and templates (P§14).
use crate::{
    app::App,
    dialogs, finder,
    finder::{FinderSource, Item, Row},
    gtk,
};
use chrono::{Local, NaiveDate};
use gtk::prelude::*;
use nib_core::{
    NoteId,
    fuzzy::{Candidate, CandidateTarget},
    template::{self, TemplateVars},
};
use std::rc::Rc;

/// The date `:daily` means: today, `+N`/`-N` days from today, or `YYYY-MM-DD`.
pub fn daily_date(arg: &str, today: NaiveDate) -> Option<NaiveDate> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Some(today);
    }
    if let Ok(days) = arg.parse::<i64>() {
        return today.checked_add_signed(chrono::Duration::try_days(days)?);
    }
    NaiveDate::parse_from_str(arg, "%Y-%m-%d").ok()
}

/// `Alt+D`, `:daily [±N|date]`, `--daily`: open (creating from the daily
/// template if needed, never overwriting) that day's note.
pub fn daily(app: &Rc<App>, arg: &str) {
    let Some(date) = daily_date(arg, Local::now().date_naive()) else {
        app.status.error("daily: today, +N, -N or YYYY-MM-DD");
        return;
    };
    match app.vault.ensure_daily(date) {
        Ok(path) => app.open_note(&path, false, None),
        Err(e) => app.status.error(&format!("daily note: {e}")),
    }
}

/// `Alt+N`, `:new [title]`, `--new TITLE`.
pub fn new_note(app: &Rc<App>, arg: &str) {
    if arg.trim().is_empty() {
        dialogs::prompt(app, "New note", "", |app, title| {
            if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
                finder::create_note(app, title.trim(), false);
            }
        });
    } else {
        finder::create_note(app, arg.trim(), false);
    }
}

fn templates_prefix(app: &App) -> String {
    format!("{}/", app.config.borrow().vault.templates.trim_matches('/'))
}

/// `Alt+Shift+T`, `:template [name]`.
pub fn template(app: &Rc<App>, arg: &str) {
    if arg.trim().is_empty() {
        finder::open(app, Templates, "");
        return;
    }
    let rel = format!("{}{}", templates_prefix(app), arg.trim());
    match NoteId::new(&rel) {
        Ok(id) => insert_template(app, &id),
        Err(_) => app.status.error(&format!("no template {arg}")),
    }
}

/// Expand a template and insert it at the cursor, or fill an empty buffer.
/// Expansion is substitution only (`{{date}}`, `{{date:FMT}}`, `{{time}}`,
/// `{{title}}`); nothing executes.
pub fn insert_template(app: &Rc<App>, id: &NoteId) {
    let path = app.vault.root().join(id.filename());
    let text = match app.vault.read_note(&path) {
        Ok(n) => n.text,
        Err(e) => {
            app.status.error(&format!("template {id}: {e}"));
            return;
        }
    };
    let tab = app.tabs.current();
    if !tab.view.is_editable() {
        app.status.error("this tab is read-only");
        return;
    }
    let vars = TemplateVars {
        now: Local::now(),
        title: tab.title(app),
        date_format: app.config.borrow().vault.daily_format.clone(),
    };
    let expanded = template::expand(&text, &vars);
    let buffer = &tab.buffer;
    buffer.begin_user_action();
    if buffer.char_count() == 0 {
        buffer.insert(&mut buffer.start_iter(), &expanded);
    } else {
        buffer.insert_at_cursor(&expanded);
    }
    buffer.end_user_action();
    tab.view.grab_focus();
}

/// The notes in the templates folder.
pub struct Templates;

impl FinderSource for Templates {
    fn name(&self) -> &'static str {
        "templates"
    }
    fn placeholder(&self) -> &'static str {
        "insert a template"
    }
    fn start(&self, app: &Rc<App>) {
        let prefix = templates_prefix(app);
        let candidates: Vec<Candidate> = app
            .index
            .borrow()
            .notes
            .values()
            .filter(|m| m.id.as_str().starts_with(&prefix))
            .map(|m| Candidate {
                label: m.id.as_str()[prefix.len()..].to_owned(),
                text: m.id.to_string(),
                target: CandidateTarget::Note(m.id.clone()),
            })
            .collect();
        if candidates.is_empty() {
            app.status.notice(&format!(
                "no templates in {prefix} (index ready: {})",
                app.index_ready.get()
            ));
        }
        finder::match_rows(app, candidates);
    }
    fn query(&self, app: &Rc<App>, text: &str) {
        finder::query(app, text);
    }
    fn accept(&self, app: &Rc<App>, row: &Row, _: &str, _: bool) {
        if let Item::Candidate(Candidate {
            target: CandidateTarget::Note(id),
            ..
        }) = &row.item
        {
            insert_template(app, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::daily_date;
    use chrono::NaiveDate;
    #[test]
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(daily_date("", d), Some(d));
        assert_eq!(daily_date("+1", d), NaiveDate::from_ymd_opt(2026, 10, 17));
        assert_eq!(daily_date("-16", d), NaiveDate::from_ymd_opt(2026, 9, 30));
        assert_eq!(
            daily_date("2026-10-01", d),
            NaiveDate::from_ymd_opt(2026, 10, 1)
        );
        assert_eq!(daily_date("soon", d), None);
    }
}
