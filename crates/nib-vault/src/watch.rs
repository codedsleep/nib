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
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        inner.emit(VaultEvent::NoteRemoved { path: path.into() });
        if let Ok(id) = inner.id(path) {
            inner.emit_index(IndexEvent::Remove(id));
        }
    }
    if let Some(p) = path.parent() {
        inner.invalidate(p);
    }
}
fn renamed(inner: &Inner, from: &Path, to: &Path) {
    let mut moves = vec![(from.to_owned(), to.to_owned())];
    if to.is_dir() {
        moves = lock(&inner.hashes)
            .keys()
            .filter_map(|p| {
                p.strip_prefix(from)
                    .ok()
                    .map(|tail| (p.clone(), to.join(tail)))
            })
            .collect();
    }
    for (from, to) in moves {
        let old = lock(&inner.hashes).remove(&from);
        if let Some(h) = old {
            lock(&inner.hashes).insert(to.clone(), h);
        }
        if from
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            inner.emit(VaultEvent::NoteRenamed {
                from: from.clone(),
                to: to.clone(),
            });
            if let (Ok(a), Ok(b)) = (inner.id(&from), inner.id(&to)) {
                inner.emit_index(IndexEvent::Renamed(a, b));
            }
            if let Ok(bytes) = std::fs::read(&to) {
                inner.changed(&to, &bytes, true);
            }
        }
    }
    if let Some(p) = from.parent() {
        inner.invalidate(p);
    }
    if let Some(p) = to.parent() {
        inner.invalidate(p);
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
                renamed(inner, from, to);
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
                .get(&path)
                .is_some_and(|(_, h)| *h == digest);
            let unchanged = lock(&inner.hashes).get(&path) == Some(&digest);
            if !own && !unchanged {
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                {
                    inner.changed(&path, &bytes, false);
                } else {
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
pub(crate) fn start(inner: &Arc<Inner>) -> Result<(), crate::VaultError> {
    let weak = Arc::downgrade(inner);
    std::thread::Builder::new().name("nib-watch-start".into()).spawn(move||{let Some(inner)=weak.upgrade()else{return;};let weak=Arc::downgrade(&inner);match notify_debouncer_full::new_debouncer(Duration::from_millis(150),None,move|r|{if let Some(inner)=weak.upgrade(){process(&inner,r);}}){Ok(mut watcher)=>{match watcher.watch(&inner.root,RecursiveMode::Recursive){Ok(())=>{*lock(&inner.watcher)=Some(watcher);},Err(e)=>{inner.fallback.store(true,Ordering::Release);inner.notice(format!("watch unavailable: {e}; sudo sysctl fs.inotify.max_user_watches=524288"));}}},Err(e)=>{inner.fallback.store(true,Ordering::Release);inner.notice(e.to_string());}}inner.watch_ready.store(true,Ordering::Release);})?;
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
