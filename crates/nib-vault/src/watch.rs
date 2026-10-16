//! Recursive 150 ms debounced change tracking and self-write suppression.
use crate::{
    events::{IndexEvent, VaultEvent},
    service::{Inner, hash, lock},
};
use notify::{
    EventKind, RecommendedWatcher, RecursiveMode,
    event::{ModifyKind, RenameMode},
};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
pub(crate) type Watcher = Debouncer<RecommendedWatcher, RecommendedCache>;
fn relevant(inner: &Inner, path: &Path) -> bool {
    path.strip_prefix(&inner.root).is_ok_and(|p| {
        !p.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(nib_core::sort::never_shown)
        })
    })
}
fn removed(inner: &Inner, path: &Path) {
    lock(&inner.hashes).remove(path);
    lock(&inner.writes).remove(path);
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        inner.emit(VaultEvent::NoteRemoved { path: path.into() });
        if let Ok(id) = inner.id(path) {
            inner.emit_index(IndexEvent::Remove(id));
        }
    }
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        && let Ok(relative) = path.strip_prefix(&inner.root)
    {
        inner.emit_index(IndexEvent::AttachmentRemove(
            relative.to_string_lossy().into_owned(),
        ));
    }
    if let Some(p) = path.parent() {
        inner.invalidate(p);
    }
}
/// Apply a completed move immediately for own ops and paired external events.
pub(crate) fn renamed(inner: &Inner, from: &Path, to: &Path, own: bool) {
    let moves = if to.is_dir() {
        lock(&inner.hashes)
            .keys()
            .filter_map(|p| {
                p.strip_prefix(from)
                    .ok()
                    .map(|tail| (p.clone(), to.join(tail)))
            })
            .collect::<Vec<_>>()
    } else {
        vec![(from.into(), to.into())]
    };
    for (source, dest) in moves {
        moved_file(inner, &source, &dest, own);
    }
    if let Some(p) = from.parent() {
        inner.invalidate(p);
    }
    if let Some(p) = to.parent() {
        inner.invalidate(p);
    }
}
fn moved_file(inner: &Inner, from: &Path, to: &Path, own: bool) {
    let target = match inner.path(to) {
        Ok(p) => p,
        Err(e) => {
            inner.notice(e.to_string());
            removed(inner, from);
            return;
        }
    };
    let source_hash = lock(&inner.hashes).remove(from);
    let old_target_hash = lock(&inner.hashes).get(&target).copied();
    let pending = lock(&inner.writes).remove(from);
    lock(&inner.writes).remove(&target);
    if own && let Some(write) = pending {
        lock(&inner.writes).insert(target.clone(), write);
    }
    let from_note = from
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    let to_note = to.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if from_note && (own || source_hash.is_some()) {
        inner.emit(VaultEvent::NoteRenamed {
            from: from.into(),
            to: to.into(),
        });
        if let Ok(a) = inner.id(from) {
            if to_note {
                if let Ok(b) = inner.id(to) {
                    inner.emit_index(IndexEvent::Renamed(a, b));
                }
            } else {
                inner.emit_index(IndexEvent::Remove(a));
            }
        }
    } else if from_note && let Ok(id) = inner.id(from) {
        inner.emit_index(IndexEvent::Remove(id));
    }
    if !from_note && let Ok(relative) = from.strip_prefix(&inner.root) {
        inner.emit_index(IndexEvent::AttachmentRemove(
            relative.to_string_lossy().into_owned(),
        ));
    }
    if to_note {
        if let Ok(bytes) = std::fs::read(&target) {
            let digest = hash(&bytes);
            let real_move = own || source_hash.is_some();
            if real_move || old_target_hash != Some(digest) {
                let replacement = !own && old_target_hash.is_some_and(|old| old != digest);
                inner.changed(&target, &bytes, real_move && !replacement);
            }
        }
    } else {
        lock(&inner.hashes).insert(target, source_hash.unwrap_or(0));
        if let Ok(relative) = to.strip_prefix(&inner.root) {
            inner.emit_index(IndexEvent::AttachmentUpsert(
                relative.to_string_lossy().into_owned(),
            ));
        }
    }
}
fn process(inner: &Inner, result: DebounceEventResult) {
    let events = match result {
        Ok(e) => e,
        Err(errors) => {
            for e in errors {
                if e.to_string().contains("No space left")
                    || matches!(e.kind, notify::ErrorKind::MaxFilesWatch)
                {
                    if !inner.fallback.swap(true, Ordering::AcqRel) {
                        inner.notice("watch limit reached; re-list on focus-in. Fix: sudo sysctl fs.inotify.max_user_watches=524288");
                    }
                } else {
                    inner.notice(e.to_string());
                }
            }
            return;
        }
    };
    let mut paths = BTreeSet::<PathBuf>::new();
    for e in events {
        if matches!(e.kind, EventKind::Access(_)) {
            continue;
        }
        if matches!(
            e.kind,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        ) && e.paths.len() == 2
        {
            let (from, to) = (&e.paths[0], &e.paths[1]);
            if relevant(inner, from) && relevant(inner, to) {
                renamed(inner, from, to, false);
                continue;
            }
        }
        for p in &e.paths {
            if relevant(inner, p) {
                paths.insert(p.clone());
            }
        }
    }
    for p in paths {
        if p.is_dir() {
            inner.invalidate(&p);
            if let Some(parent) = p.parent() {
                inner.invalidate(parent);
            }
            continue;
        }
        if let Ok(path) = inner.path(&p)
            && let Ok(bytes) = std::fs::read(&path)
        {
            let digest = hash(&bytes);
            let own = lock(&inner.writes)
                .remove(&path)
                .is_some_and(|(_, h)| h == digest);
            let unchanged = lock(&inner.hashes).get(&path) == Some(&digest);
            if !own && !unchanged {
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                {
                    inner.changed(&path, &bytes, false);
                } else {
                    if let Ok(relative) = path.strip_prefix(&inner.root) {
                        inner.emit_index(IndexEvent::AttachmentUpsert(
                            relative.to_string_lossy().into_owned(),
                        ));
                    }
                    lock(&inner.hashes).insert(path, digest);
                    if let Some(p) = p.parent() {
                        inner.invalidate(p);
                    }
                }
            }
            continue;
        }
        if !p.exists() {
            removed(inner, &p);
        }
    }
}
fn install(weak: &std::sync::Weak<Inner>) {
    let Some(inner) = weak.upgrade() else {
        return;
    };
    if inner.stopped.load(Ordering::Acquire) {
        return;
    }
    if let Err(e) =
        crate::fsx::cleanup_stale_tmp(&inner.guard, &inner.root, std::time::SystemTime::now())
    {
        inner.notice(e.to_string());
    }
    if inner.stopped.load(Ordering::Acquire) {
        return;
    }
    let weak = Arc::downgrade(&inner);
    let callback = move |result| {
        if let Some(inner) = weak.upgrade() {
            process(&inner, result);
        }
    };
    match notify_debouncer_full::new_debouncer(Duration::from_millis(150), None, callback) {
        Ok(mut watcher) => match watcher.watch(&inner.root, RecursiveMode::Recursive) {
            Ok(()) => *lock(&inner.watcher) = Some(watcher),
            Err(e) => {
                inner.fallback.store(true, Ordering::Release);
                inner.notice(format!(
                    "watch unavailable: {e}; sudo sysctl fs.inotify.max_user_watches=524288"
                ));
            }
        },
        Err(e) => {
            inner.fallback.store(true, Ordering::Release);
            inner.notice(e.to_string());
        }
    }
    inner.watch_ready.store(true, Ordering::Release);
}
pub(crate) fn start(inner: &Arc<Inner>) -> Result<(), crate::VaultError> {
    let weak = Arc::downgrade(inner);
    std::thread::Builder::new()
        .name("nib-watch-start".into())
        .spawn(move || install(&weak))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watch_limit_notice_once() {
        let d = tempfile::tempdir().unwrap();
        let (h, rx) = crate::VaultHandle::open(
            d.path(),
            &nib_core::settings::Config::default(),
            crate::Guard::confined(vec![d.path().into()]),
        )
        .unwrap();
        for _ in 0..2 {
            process(
                &h.inner,
                Err(vec![notify::Error::new(notify::ErrorKind::MaxFilesWatch)]),
            );
        }
        assert!(h.needs_relist());
        let mut notices = 0;
        while let Ok(e) = rx.try_recv() {
            if let VaultEvent::Notice(n) = e {
                assert!(n.relist_on_focus);
                assert!(n.message.contains("sysctl fs.inotify.max_user_watches"));
                notices += 1;
            }
        }
        assert_eq!(notices, 1);
    }
}
