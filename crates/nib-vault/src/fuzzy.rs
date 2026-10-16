//! Event-driven nucleo ownership; the UI consumes immutable snapshot messages.
use crate::VaultError;
use nib_core::fuzzy::Candidate;
use nucleo::{
    Config, Nucleo, Utf32String,
    pattern::{CaseMatching, Normalization},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
/// Immutable ranked result page from one query generation.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Generation returned by `Matcher::set_query`; zero is the empty query.
    pub query: u64,
    /// Matching candidates in score order, at most the requested page size.
    pub candidates: Vec<Candidate>,
    /// Total matching rows.
    pub matched: u32,
    /// Further ranked snapshots will arrive when workers finish.
    pub running: bool,
}
enum Command {
    Query(u64, String),
    Wake,
    Stop,
}
/// A matcher actor. Injection, ticking and nucleo's thread pool stay in vault;
/// dropping this handle stops the actor and releases the pool without UI polling.
pub struct Matcher {
    commands: async_channel::Sender<Command>,
    events: async_channel::Receiver<Snapshot>,
    next: AtomicU64,
}
impl Matcher {
    /// Inject a prebuilt source in its empty-query order, asynchronously.
    pub fn new(candidates: impl IntoIterator<Item = Candidate>) -> Result<Self, VaultError> {
        let candidates: Vec<_> = candidates.into_iter().collect();
        let (commands, rx) = async_channel::unbounded();
        let (tx, events) = async_channel::unbounded();
        let wake = commands.clone();
        std::thread::Builder::new()
            .name("nib-match".into())
            .spawn(move || {
                let dirty = Arc::new(AtomicBool::new(false));
                let signal = dirty.clone();
                let notify = Arc::new(move || {
                    if !signal.swap(true, Ordering::AcqRel) {
                        let _ = wake.try_send(Command::Wake);
                    }
                });
                let mut config = Config::DEFAULT;
                config.set_match_paths();
                let mut nucleo = Nucleo::new(config, notify, None, 1);
                let injector = nucleo.injector();
                for c in candidates {
                    let text = c.text.clone();
                    injector.push(c, |_, columns| {
                        columns[0] = Utf32String::from(text);
                    });
                }
                let mut query = 0;
                while let Ok(command) = rx.recv_blocking() {
                    let mut command = Some(command);
                    let mut latest = None;
                    loop {
                        match command.take() {
                            Some(Command::Stop) => return,
                            Some(Command::Query(id, text)) => latest = Some((id, text)),
                            _ => {}
                        }
                        match rx.try_recv() {
                            Ok(c) => command = Some(c),
                            Err(_) => break,
                        }
                    }
                    if let Some((id, text)) = latest {
                        query = id;
                        nucleo.pattern.reparse(
                            0,
                            &text,
                            CaseMatching::Smart,
                            Normalization::Smart,
                            false,
                        );
                    }
                    dirty.store(false, Ordering::Release);
                    let status = nucleo.tick(1);
                    let snapshot = nucleo.snapshot();
                    let matched = snapshot.matched_item_count();
                    let candidates = snapshot
                        .matched_items(0..14.min(matched))
                        .map(|item| item.data.clone())
                        .collect();
                    if tx
                        .try_send(Snapshot {
                            query,
                            candidates,
                            matched,
                            running: status.running,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            })?;
        let _ = commands.try_send(Command::Wake);
        Ok(Self {
            commands,
            events,
            next: AtomicU64::new(1),
        })
    }
    /// Submit an fzf query and return its generation for discarding obsolete pages.
    pub fn set_query(&self, query: &str) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let _ = self.commands.try_send(Command::Query(id, query.into()));
        id
    }
    /// Snapshot stream; consume from one receiver on the main context.
    pub fn events(&self) -> async_channel::Receiver<Snapshot> {
        self.events.clone()
    }
}
impl Drop for Matcher {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Stop);
        self.events.close();
    }
}
