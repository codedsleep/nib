//! Single listing worker and twelve-folder LRU of immutable rows.
use crate::{
    VaultError,
    events::VaultEvent,
    service::{Inner, lock},
};
use nib_core::sort::{Entry, never_shown, sort_entries};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::SystemTime,
};
#[derive(Default)]
pub(crate) struct ListingCache {
    entries: VecDeque<(PathBuf, Arc<[Entry]>, bool)>,
    pub reads: usize,
}
impl ListingCache {
    pub fn invalidate(&mut self, path: &Path) {
        if let Some((_, _, dirty)) = self.entries.iter_mut().find(|(p, _, _)| p == path) {
            *dirty = true;
        }
    }
    fn get(&mut self, path: &Path) -> Option<Arc<[Entry]>> {
        let i = self
            .entries
            .iter()
            .position(|(p, _, dirty)| p == path && !*dirty)?;
        let item = self.entries.remove(i)?;
        let rows = item.1.clone();
        self.entries.push_front(item);
        Some(rows)
    }
    fn contains(&self, path: &Path) -> bool {
        self.entries.iter().any(|(p, _, _)| p == path)
    }
    fn insert(&mut self, path: PathBuf, rows: Arc<[Entry]>) {
        self.entries.retain(|(p, _, _)| *p != path);
        self.entries.push_front((path, rows, false));
        self.entries.truncate(12);
        self.reads += 1;
    }
}
pub(crate) struct ListRequest {
    pub folder: PathBuf,
    pub force: bool,
    pub only_cached: bool,
}
fn read(folder: &Path) -> Result<Arc<[Entry]>, VaultError> {
    let mut rows = Vec::new();
    for e in std::fs::read_dir(folder)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if never_shown(&name) {
            continue;
        }
        let m = std::fs::metadata(e.path()).or_else(|_| e.metadata())?;
        rows.push(Entry {
            path: e.path(),
            name,
            directory: m.is_dir(),
            modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: m.len(),
        });
    }
    sort_entries(&mut rows, nib_core::settings::Sort::Name, false);
    Ok(rows.into())
}
pub(crate) fn start(
    inner: &Arc<Inner>,
    rx: async_channel::Receiver<ListRequest>,
) -> Result<(), VaultError> {
    let weak: Weak<Inner> = Arc::downgrade(inner);
    std::thread::Builder::new()
        .name("nib-list".into())
        .spawn(move || {
            if let Some(inner) = weak.upgrade() {
                let _ = crate::fsx::cleanup_stale_tmp(&inner.guard, &inner.root, SystemTime::now());
            }
            while let Ok(req) = rx.recv_blocking() {
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                if req.only_cached && !lock(&inner.cache).contains(&req.folder) {
                    continue;
                }
                let cached = if req.force {
                    None
                } else {
                    lock(&inner.cache).get(&req.folder)
                };
                let disk = cached.is_none();
                match cached.map_or_else(|| read(&req.folder), Ok) {
                    Ok(rows) => {
                        if disk {
                            lock(&inner.cache).insert(req.folder.clone(), rows.clone());
                        }
                        inner.emit(VaultEvent::Listing {
                            folder: req.folder,
                            entries: rows,
                        });
                    }
                    Err(e) => inner.notice(e.to_string()),
                }
            }
        })?;
    Ok(())
}
