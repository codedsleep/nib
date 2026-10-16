//! Temporary CLI shell; the GTK window lands in Goal B.
#![forbid(unsafe_code)]
fn main() {
    nib_vault::log::init();
    let invocation = match nib_core::cli::parse(std::env::args().skip(1)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("nib: {e}");
            std::process::exit(2);
        }
    };
    let paths = nib_vault::state::paths(invocation.profile.as_deref());
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Panic payloads may contain note contents: record location and backtrace only.
        let report = format!(
            "nib {} panic at {:?}\nGTK/GtkSourceView: not loaded (stub); vault key: none\n{}\n",
            env!("CARGO_PKG_VERSION"),
            info.location(),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = nib_vault::state::append_crash(&paths, &nib_vault::Guard::from_env(), &report);
        hook(info);
    }));
    assert!(
        !(cfg!(debug_assertions) && std::env::var_os("NIB_TEST_PANIC").is_some()),
        "forced crash-hook test"
    );
    if invocation.version {
        println!("nib {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if invocation.help {
        println!("{}", nib_core::cli::USAGE);
        return;
    }
    eprintln!("nib: the window arrives in Phase 5");
    std::process::exit(2);
}
