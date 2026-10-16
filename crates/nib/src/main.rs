//! nib: a GTK 4 Markdown notes app for Obsidian vaults.
#![forbid(unsafe_code)]
use gtk::prelude::*;
use gtk4 as gtk;
use std::sync::Mutex;

mod app;
mod commands;
mod debug;
mod dialogs;
mod editor;
mod files;
mod help;
mod launch;
mod panel;
mod registry;
mod status;
mod tabs;
mod theme;

/// Base application id; a profile appends `.p_NAME`.
pub const APP_ID: &str = "io.github.codedsleep.Nib";

/// Vault key of the open window, for the crash report.
pub static CRASH_VAULT_KEY: Mutex<Option<String>> = Mutex::new(None);

fn main() -> glib::ExitCode {
    let t0 = std::time::Instant::now();
    nib_vault::log::init();
    let invocation = match nib_core::cli::parse(std::env::args().skip(1)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("nib: {e}");
            return glib::ExitCode::from(2);
        }
    };
    let paths = nib_vault::state::paths(invocation.profile.as_deref());
    install_panic_hook(paths.clone());
    assert!(
        !(cfg!(debug_assertions) && std::env::var_os("NIB_TEST_PANIC").is_some()),
        "forced crash-hook test"
    );
    if invocation.version {
        println!("nib {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if invocation.help {
        println!("{}", nib_core::cli::USAGE);
        return glib::ExitCode::SUCCESS;
    }
    if !cfg!(debug_assertions) && (invocation.test_keys.is_some() || invocation.test_dump) {
        eprintln!("nib: --test-keys and --test-dump exist only in debug builds");
        return glib::ExitCode::from(2);
    }
    let id = match &invocation.profile {
        Some(p) => format!("{APP_ID}.p_{p}"),
        None => APP_ID.to_owned(),
    };
    let application = gtk::Application::builder()
        .application_id(id)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let launcher = app::Launcher::new(t0, paths, invocation.profile);
    application.connect_command_line({
        let launcher = launcher.clone();
        move |gapp, cmd| launcher.command_line(gapp, cmd)
    });
    application.connect_open({
        let launcher = launcher.clone();
        move |gapp, files, _| launcher.open_files(gapp, files)
    });
    let status = application.run_with_args(&std::env::args().collect::<Vec<_>>());
    launcher.shutdown();
    launcher.exit_status().unwrap_or(status)
}

fn install_panic_hook(paths: nib_vault::state::Paths) {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Panic payloads may contain note contents: record location and backtrace only.
        let key = CRASH_VAULT_KEY
            .lock()
            .ok()
            .and_then(|k| k.clone())
            .unwrap_or_else(|| "none".into());
        // GtkSourceView's version getters assert an initialised main thread.
        let toolkit = if gtk::is_initialized_main_thread() {
            format!(
                "GTK {}.{}.{}, GtkSourceView {}.{}.{}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
                sourceview5::major_version(),
                sourceview5::minor_version(),
                sourceview5::micro_version()
            )
        } else {
            format!(
                "GTK {}.{}.{} (not initialised on this thread)",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version()
            )
        };
        let report = format!(
            "nib {} panic at {:?}\n{toolkit}; vault key: {key}\n{}\n",
            env!("CARGO_PKG_VERSION"),
            info.location(),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = nib_vault::state::append_crash(&paths, &nib_vault::Guard::from_env(), &report);
        hook(info);
    }));
}
