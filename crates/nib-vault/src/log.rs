//! Small stderr logger and opt-in elapsed-time instrumentation.
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::max_level()
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: Logger = Logger;
/// Install stderr logging once. Never log note contents.
pub fn init() {
    let level = match std::env::var("NIB_LOG").as_deref() {
        Ok("debug") => log::LevelFilter::Debug,
        Ok("info") => log::LevelFilter::Info,
        _ => log::LevelFilter::Warn,
    };
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }
}
/// A timed scope, printed when `NIB_TRACE=1`.
pub struct Span {
    name: &'static str,
    start: std::time::Instant,
}
impl Span {
    /// Begin a named measurement.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            start: std::time::Instant::now(),
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if std::env::var("NIB_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "name={} ms={:.3}",
                self.name,
                self.start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}
/// Bind a trace span for the enclosing scope.
#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {
        let _nib_trace_span = $crate::log::Span::new($name);
    };
}
