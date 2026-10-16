//! Measurements (`NIB_TRACE=1`) and the debug-only hooks the headless checks
//! drive: `--test-dump` (JSON state) and `--test-keys` (real key events).
use crate::{app::App, gtk};
use gtk::prelude::*;
use std::{
    cell::Cell,
    fmt::Write as _,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn tracing() -> bool {
    std::env::var("NIB_TRACE").as_deref() == Ok("1")
}

/// Milliseconds since process start: from `NIB_T0` (ns since the epoch, set by
/// the harness just before exec) when present, else from `main`.
fn since_start(t0: Instant) -> f64 {
    if let Some(ns) = std::env::var("NIB_T0")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        && let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH)
    {
        let since = now.saturating_sub(Duration::from_nanos(ns));
        return since.as_secs_f64() * 1000.0;
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

pub fn trace_since(t0: Instant, name: &str) {
    if tracing() {
        eprintln!("name={name} ms={:.3}", since_start(t0));
    }
}

pub fn trace_ms(name: &str, started: Instant) {
    if tracing() {
        eprintln!(
            "name={name} ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Print `name` once the widget's next frame has been painted.
pub fn trace_paint(widget: &impl IsA<gtk::Widget>, name: &'static str, started: Instant) {
    if !tracing() {
        return;
    }
    let Some(clock) = widget.frame_clock() else {
        return;
    };
    let done = Rc::new(Cell::new(None::<glib::SignalHandlerId>));
    let handler = clock.connect_after_paint({
        let done = done.clone();
        move |clock| {
            trace_ms(name, started);
            if let Some(id) = done.take() {
                clock.disconnect(id);
            }
        }
    });
    done.set(Some(handler));
    widget.queue_draw();
}

/// `start_to_map` at the first painted frame, plus `nib ready` for the harness.
pub fn probe_first_frame(app: &Rc<App>) {
    let t0 = app.t0;
    app.window.connect_map(move |window| {
        let Some(clock) = window.frame_clock() else {
            return;
        };
        let done = Rc::new(Cell::new(None::<glib::SignalHandlerId>));
        let handler = clock.connect_after_paint({
            let done = done.clone();
            move |clock| {
                trace_since(t0, "start_to_map");
                if tracing() {
                    eprintln!("nib ready");
                }
                if let Some(id) = done.take() {
                    clock.disconnect(id);
                }
            }
        });
        done.set(Some(handler));
    });
    stall_probe();
    frame_probe(app);
}

/// `NIB_FRAMES=1`: print the work of every frame (before-paint to
/// after-paint, which includes layout and rendering), for latency.sh.
fn frame_probe(app: &Rc<App>) {
    if std::env::var("NIB_FRAMES").as_deref() != Ok("1") {
        return;
    }
    app.window.connect_realize(|window| {
        let Some(clock) = window.frame_clock() else {
            return;
        };
        let began = Rc::new(Cell::new(None::<Instant>));
        clock.connect_before_paint({
            let began = began.clone();
            move |_| began.set(Some(Instant::now()))
        });
        clock.connect_after_paint(move |_| {
            if let Some(t) = began.take() {
                eprintln!("frame ms={:.3}", t.elapsed().as_secs_f64() * 1000.0);
            }
        });
    });
}

/// Dev-only (debug build with `NIB_TRACE=1`): a thread pings the main context
/// at high priority and reports any dispatch that kept it busy over 4 ms.
fn stall_probe() {
    if !cfg!(debug_assertions) || !tracing() {
        return;
    }
    let context = glib::MainContext::default();
    let _ = std::thread::Builder::new()
        .name("nib-stall".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let sent = Instant::now();
                context.invoke_with_priority(glib::Priority::HIGH, move || {
                    let late = sent.elapsed();
                    if late > Duration::from_millis(4) {
                        eprintln!(
                            "stall: main thread busy {:.1} ms",
                            late.as_secs_f64() * 1000.0
                        );
                    }
                });
            }
        });
}

/// `--test-keys "key j type abc"`: hand the words to the agent's virtual
/// keyboard (`NIB_VINPUT`), the same path a person's keys take.
pub fn inject(keys: &str) -> glib::ExitCode {
    let Some(tool) = std::env::var_os("NIB_VINPUT") else {
        eprintln!("nib: --test-keys needs NIB_VINPUT (scripts/agent/agent-run.sh sets it)");
        return glib::ExitCode::from(2);
    };
    match std::process::Command::new(tool)
        .args(keys.split_whitespace())
        .status()
    {
        Ok(s) if s.success() => glib::ExitCode::SUCCESS,
        _ => glib::ExitCode::from(1),
    }
}

/// A JSON string literal.
pub fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list(items: impl IntoIterator<Item = String>) -> String {
    let items: Vec<String> = items.into_iter().collect();
    format!("[{}]", items.join(","))
}

/// Which region holds the keyboard.
pub fn focus_region(app: &App) -> &'static str {
    let Some(focus) = gtk::prelude::GtkWindowExt::focus(&app.window) else {
        return "none";
    };
    if focus.is_ancestor(&app.files.root) {
        return "files";
    }
    if focus.is_ancestor(&app.right) {
        return "panel";
    }
    if focus.has_css_class("nib-reading") {
        return "reading";
    }
    if focus.is::<sourceview5::View>() {
        return "editor";
    }
    "other"
}

/// The state the headless checks assert on.
pub fn dump(app: &Rc<App>) -> String {
    let tabs = app.tabs.all();
    let active = app.tabs.active_index();
    let tab_json = tabs.iter().enumerate().map(|(i, t)| {
        format!(
            "{{\"title\":{},\"path\":{},\"modified\":{},\"active\":{},\"read_only\":{}}}",
            json(&t.title(app)),
            json(&t.display_path(app)),
            t.buffer.is_modified(),
            i == active,
            t.read_only.get(),
        )
    });
    let tab = app.tabs.current();
    let cursor = tab.buffer.iter_at_mark(&tab.buffer.get_insert());
    let status = app.status.texts();
    let files_folder = app.relative(&app.files.folder());
    format!(
        concat!(
            "{{\"tabs\":{},\"cursor\":{{\"line\":{},\"column\":{}}},",
            "\"files\":{{\"folder\":{},\"cursor\":{},\"rows\":{}}},",
            "\"status\":{{\"mode\":{},\"pending\":{},\"path\":{},\"words\":{},\"position\":{},\"right\":{}}},",
            "\"focus\":{},\"focus_widget\":{},\"panels\":{{\"files\":{},\"right\":{}}},\"index_ready\":{},\"title\":{},",
            "\"vault_key\":{},\"keys_shown\":{}}}\n"
        ),
        list(tab_json),
        cursor.line() + 1,
        cursor.line_offset() + 1,
        json(&files_folder),
        json(&app.files.cursor_name().unwrap_or_default()),
        app.files.store.n_items(),
        json(&status[0]),
        json(&status[1]),
        json(&status[2]),
        json(&status[3]),
        json(&status[4]),
        json(&status[5]),
        json(focus_region(app)),
        json(
            &gtk::prelude::GtkWindowExt::focus(&app.window)
                .map(|w| w.type_().name().to_owned())
                .unwrap_or_default()
        ),
        app.files.root.is_visible(),
        app.right.is_visible(),
        app.index_ready.get(),
        json(&app.window.title().unwrap_or_default()),
        json(&app.key),
        crate::help::shown(app).map_or_else(|| "null".into(), |n| n.to_string()),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_escapes() {
        assert_eq!(super::json("a\"b\\c\n\u{1}"), "\"a\\\"b\\\\c\\n\\u0001\"");
    }
}
