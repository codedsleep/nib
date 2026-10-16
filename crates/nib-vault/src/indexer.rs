//! Ignore-aware traversal and chunked parallel note parsing.
use crate::{
    events::{IndexEvent, VaultEvent},
    service::{Inner, hash, lock},
};
use ignore::WalkBuilder;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
pub(crate) fn walker(root: &Path) -> WalkBuilder {
    let mut b = WalkBuilder::new(root);
    b.hidden(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| {
            e.depth() == 0
                || !e
                    .file_name()
                    .to_str()
                    .is_some_and(nib_core::sort::never_shown)
        });
    b
}
#[allow(clippy::too_many_lines)] // Worker setup and its scoped execution share cancellation state.
pub(crate) fn start(inner: &Arc<Inner>) {
    let updates = lock(&inner.index_updates);
    if inner.indexing.swap(true, Ordering::AcqRel) {
        return;
    }
    drop(updates);
    let weak = Arc::downgrade(inner);
    if let Err(e) = std::thread::Builder::new()
        .name("nib-index".into())
        .spawn(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let start = Instant::now();
            let mut paths = Vec::new();
            let mut attachments = Vec::new();
            for e in walker(&inner.root).build().filter_map(Result::ok) {
                if inner.stopped.load(Ordering::Acquire) {
                    return;
                }
                if e.path().is_file() {
                    if e.path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("md"))
                    {
                        paths.push(e.into_path());
                    } else if inner.path(e.path()).is_ok()
                        && let Ok(p) = e.path().strip_prefix(&inner.root)
                    {
                        lock(&inner.hashes).entry(e.path().into()).or_insert(0);
                        attachments.push(p.to_string_lossy().into_owned());
                    }
                }
            }
            let count = paths.len();
            let next = AtomicUsize::new(0);
            let done = AtomicUsize::new(0);
            let progress = std::sync::Mutex::new(Instant::now());
            let workers = std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(count.div_ceil(32).max(1));
            let metas = std::thread::scope(|scope| {
                let mut joins = Vec::new();
                for _ in 0..workers {
                    let (paths, next, done, progress, inner) =
                        (&paths, &next, &done, &progress, &inner);
                    joins.push(scope.spawn(move || {
                        let mut metas = Vec::new();
                        loop {
                            let start = next.fetch_add(32, Ordering::Relaxed);
                            if start >= paths.len() || inner.stopped.load(Ordering::Acquire) {
                                break;
                            }
                            for p in &paths[start..(start + 32).min(paths.len())] {
                                if let Ok(path) = inner.path(p)
                                    && let (Ok(bytes), Ok(m)) =
                                        (std::fs::read(&path), std::fs::metadata(&path))
                                {
                                    lock(&inner.hashes).insert(path.clone(), hash(&bytes));
                                    if let Ok(id) = inner.id(p) {
                                        let text = nib_core::text::decode(&bytes)
                                            .map_or_else(|_| String::new(), |(text, _)| text);
                                        metas.push(nib_core::parse_note(
                                            id,
                                            &text,
                                            m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                                            m.len(),
                                        ));
                                    }
                                }
                                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                                let mut last = lock(progress);
                                if last.elapsed() >= Duration::from_millis(50) {
                                    inner.emit(VaultEvent::Index(IndexEvent::Progress(n, count)));
                                    *last = Instant::now();
                                }
                            }
                        }
                        metas
                    }));
                }
                joins
                    .into_iter()
                    .filter_map(|j| j.join().ok())
                    .flatten()
                    .collect()
            });
            let mut index = nib_core::Index::build_with_attachments(metas, attachments);
            let mut updates = lock(&inner.index_updates);
            for event in updates.drain(..) {
                match event {
                    IndexEvent::Upsert(meta) => index.upsert(*meta),
                    IndexEvent::Remove(id) => index.remove(&id),
                    IndexEvent::Renamed(a, b) => index.rename(&a, b),
                    IndexEvent::AttachmentUpsert(path) => index.upsert_attachment(&path),
                    IndexEvent::AttachmentRemove(path) => index.remove_attachment(&path),
                    _ => {}
                }
            }
            inner.emit(VaultEvent::Index(IndexEvent::Ready(Box::new(index))));
            inner.indexing.store(false, Ordering::Release);
            drop(updates);
            if std::env::var("NIB_TRACE").as_deref() == Ok("1") {
                eprintln!(
                    "name=index_ready ms={:.3} notes={count}",
                    start.elapsed().as_secs_f64() * 1000.0
                );
            }
        })
    {
        inner.indexing.store(false, Ordering::Release);
        inner.notice(e.to_string());
    }
}
