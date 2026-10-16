//! Cancellable parallel grep and streaming recursive finder walks.
use crate::{
    events::{RequestId, SearchEvent, SearchHit, VaultEvent, WalkEvent},
    service::{Inner, lock},
};
use nib_core::search::SearchSpec;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
#[derive(Default)]
struct Cancellation {
    cancelled: Mutex<bool>,
}
impl Cancellation {
    fn stopped(&self) -> bool {
        *lock(&self.cancelled)
    }
    fn send(&self, inner: &Inner, event: VaultEvent) {
        let cancelled = lock(&self.cancelled);
        if !*cancelled {
            inner.emit(event);
        }
    }
    fn cancel(&self) {
        *lock(&self.cancelled) = true;
    }
}
/// Running search. Cancellation synchronizes with event emission; queued old
/// events retain this ID so the consumer can discard them after a query change.
pub struct SearchHandle {
    id: RequestId,
    cancel: Arc<Cancellation>,
}
impl SearchHandle {
    /// Request identity attached to all results.
    pub fn id(&self) -> RequestId {
        self.id
    }
    /// Stop future sends before returning.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}
impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
/// Recursive walk with the same cancellation contract as a search.
pub struct WalkHandle {
    id: RequestId,
    cancel: Arc<Cancellation>,
}
impl WalkHandle {
    /// Request identity.
    pub fn id(&self) -> RequestId {
        self.id
    }
    /// Stop future sends.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}
impl Drop for WalkHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[allow(clippy::too_many_lines)] // Worker setup and its scoped execution share cancellation state.
pub(crate) fn start(inner: &Arc<Inner>, spec: SearchSpec) -> SearchHandle {
    let id = inner.request_id();
    let cancel = Arc::new(Cancellation::default());
    let handle = SearchHandle {
        id,
        cancel: cancel.clone(),
    };
    let weak = Arc::downgrade(inner);
    if let Err(e) = std::thread::Builder::new()
        .name("nib-search".into())
        .spawn(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let mut builder = grep_regex::RegexMatcherBuilder::new();
            builder
                .fixed_strings(!spec.regex)
                .case_insensitive(!spec.case_sensitive);
            let matcher = match builder.build(&spec.pattern) {
                Ok(m) => m,
                Err(e) => {
                    cancel.send(
                        &inner,
                        VaultEvent::Search(SearchEvent::Failed(id, e.to_string())),
                    );
                    return;
                }
            };
            if spec.pattern.is_empty() {
                cancel.send(&inner, VaultEvent::Search(SearchEvent::Done(id)));
                return;
            }
            let count = AtomicUsize::new(0);
            let more = AtomicBool::new(false);
            let mut walk = crate::indexer::walker(&inner.root);
            walk.threads(std::thread::available_parallelism().map_or(1, usize::from));
            walk.build_parallel().run(|| {
                let (matcher, cancel, inner, spec, count, more) =
                    (&matcher, &cancel, &inner, &spec, &count, &more);
                Box::new(move |entry| {
                    if cancel.stopped()
                        || inner.stopped.load(Ordering::Acquire)
                        || more.load(Ordering::Acquire)
                    {
                        return ignore::WalkState::Quit;
                    }
                    let Ok(entry) = entry else {
                        return ignore::WalkState::Continue;
                    };
                    let p = entry.path();
                    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
                        || !p.is_file()
                        || spec.excludes.iter().any(|x| {
                            p.strip_prefix(&inner.root)
                                .is_ok_and(|r| r.to_string_lossy().contains(x))
                        })
                    {
                        return ignore::WalkState::Continue;
                    }
                    let Ok(path) = inner.path(p) else {
                        return ignore::WalkState::Continue;
                    };
                    let Ok(note) = inner.id(p) else {
                        return ignore::WalkState::Continue;
                    };
                    let mut hits = Vec::new();
                    let mut searcher = grep_searcher::SearcherBuilder::new()
                        .line_number(true)
                        .build();
                    let result = searcher.search_path(
                        matcher,
                        &path,
                        grep_searcher::sinks::UTF8(|line, text| {
                            if cancel.stopped() {
                                return Ok(false);
                            }
                            let n = count.fetch_add(1, Ordering::AcqRel);
                            if n >= 2000 {
                                if !more.swap(true, Ordering::AcqRel) {
                                    cancel.send(inner, VaultEvent::Search(SearchEvent::More(id)));
                                }
                                return Ok(false);
                            }
                            hits.push(SearchHit {
                                note: note.clone(),
                                line,
                                text: text.trim_end_matches(['\r', '\n']).into(),
                            });
                            if hits.len() >= 32 {
                                cancel.send(
                                    inner,
                                    VaultEvent::Search(SearchEvent::Batch {
                                        id,
                                        hits: std::mem::take(&mut hits),
                                    }),
                                );
                            }
                            Ok(true)
                        }),
                    );
                    if let Err(e) = result {
                        log::debug!("search skipped {}: {e}", path.display());
                    }
                    if !hits.is_empty() {
                        cancel.send(inner, VaultEvent::Search(SearchEvent::Batch { id, hits }));
                    }
                    ignore::WalkState::Continue
                })
            });
            cancel.send(&inner, VaultEvent::Search(SearchEvent::Done(id)));
        })
    {
        inner.notice(e.to_string());
    }
    handle
}
pub(crate) fn walk(inner: &Arc<Inner>, folder: &Path) -> WalkHandle {
    let id = inner.request_id();
    let cancel = Arc::new(Cancellation::default());
    let handle = WalkHandle {
        id,
        cancel: cancel.clone(),
    };
    let folder = match inner.path(folder) {
        Ok(p) => p,
        Err(e) => {
            inner.notice(e.to_string());
            return handle;
        }
    };
    let weak = Arc::downgrade(inner);
    if let Err(e) = std::thread::Builder::new()
        .name("nib-walk".into())
        .spawn(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let mut paths = Vec::new();
            for e in crate::indexer::walker(&folder)
                .build()
                .filter_map(Result::ok)
            {
                if cancel.stopped() || inner.stopped.load(Ordering::Acquire) {
                    return;
                }
                if e.depth() == 0 {
                    continue;
                }
                if let Ok(p) = e.path().strip_prefix(&inner.root) {
                    let mut path = p.to_string_lossy().into_owned();
                    if e.file_type().is_some_and(|t| t.is_dir()) {
                        path.push('/');
                    }
                    paths.push(path);
                    if paths.len() >= 128 {
                        cancel.send(
                            &inner,
                            VaultEvent::Walk(WalkEvent::Batch {
                                id,
                                paths: std::mem::take(&mut paths),
                            }),
                        );
                    }
                }
            }
            if !paths.is_empty() {
                cancel.send(&inner, VaultEvent::Walk(WalkEvent::Batch { id, paths }));
            }
            cancel.send(&inner, VaultEvent::Walk(WalkEvent::Done(id)));
        })
    {
        inner.notice(e.to_string());
    }
    handle
}
