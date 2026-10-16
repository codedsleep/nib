//! Worker lifetimes, incremental watches, search, listing and file ops.
use nib_core::{NoteId, search::SearchSpec, settings::Config};
use nib_vault::{ops::FileOp, *};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}
fn open(root: &Path) -> (VaultHandle, async_channel::Receiver<VaultEvent>) {
    VaultHandle::open(root, &Config::default(), Guard::confined(vec![root.into()])).unwrap()
}
fn until<T>(
    rx: &async_channel::Receiver<VaultEvent>,
    mut f: impl FnMut(VaultEvent) -> Option<T>,
) -> T {
    let start = Instant::now();
    loop {
        while let Ok(e) = rx.try_recv() {
            if let Some(value) = f(e) {
                return value;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "event deadline exceeded"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn watch(h: &VaultHandle) {
    let start = Instant::now();
    while !h.watcher_ready() {
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn op(
    h: &VaultHandle,
    rx: &async_channel::Receiver<VaultEvent>,
    op: FileOp,
    answer: Option<ConflictAnswer>,
) -> Result<OpSummary, String> {
    let token = h.op(op);
    until(rx, |e| match e {
        VaultEvent::Op(OpEvent::Conflict { id, .. }) if id == token => {
            h.answer(id, answer.expect("unexpected conflict"));
            None
        }
        VaultEvent::Op(OpEvent::Done { id, summary }) if id == token => Some(Ok(summary)),
        VaultEvent::Op(OpEvent::Failed { id, message }) if id == token => Some(Err(message)),
        _ => None,
    })
}
#[test]
fn cached_listing_and_ignore_index() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("a.md"), "# A\n#tag").unwrap();
    fs::write(d.path().join("skip.md"), "skip").unwrap();
    fs::write(d.path().join(".gitignore"), "skip.md\n").unwrap();
    fs::create_dir(d.path().join(".obsidian")).unwrap();
    fs::write(d.path().join(".obsidian/secret.md"), "secret").unwrap();
    let (h, rx) = open(d.path());
    watch(&h);
    h.list(d.path());
    let first = until(&rx, |e| {
        if let VaultEvent::Listing { entries, .. } = e {
            Some(entries)
        } else {
            None
        }
    });
    h.list(d.path());
    let second = until(&rx, |e| {
        if let VaultEvent::Listing { entries, .. } = e {
            Some(entries)
        } else {
            None
        }
    });
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(h.listing_reads(), 1);
    assert!(first.iter().all(|e| e.name != ".obsidian"));
    h.reread(d.path());
    until(&rx, |e| {
        matches!(e, VaultEvent::Listing { .. }).then_some(())
    });
    assert_eq!(h.listing_reads(), 2);
    h.start_index();
    let i = until(&rx, |e| {
        if let VaultEvent::Index(IndexEvent::Ready(i)) = e {
            Some(i)
        } else {
            None
        }
    });
    assert_eq!(i.notes.len(), 1);
    assert!(i.notes.contains_key(&id("a")));
}
#[test]
fn watches_external_own_touch_and_rename() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.md");
    fs::write(&p, "old\n").unwrap();
    let (h, rx) = open(d.path());
    watch(&h);
    let note = h.read_note(&p).unwrap();
    h.save(&p, "own\n", &note.fmt).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(e) = rx.try_recv() {
        assert!(
            !matches!(e, VaultEvent::NoteChanged { .. }),
            "own save became an external edit"
        );
    }
    fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(std::time::SystemTime::now())
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(e) = rx.try_recv() {
        assert!(!matches!(e, VaultEvent::NoteChanged { .. }));
    }
    for n in 0..50 {
        fs::write(&p, format!("external {n}\n")).unwrap();
    }
    let text = until(&rx, |e| {
        if let VaultEvent::NoteChanged { text, .. } = e {
            Some(text)
        } else {
            None
        }
    });
    assert_eq!(text, "external 49\n");
    std::thread::sleep(Duration::from_millis(400));
    let extra = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|e| matches!(e, VaultEvent::NoteChanged { .. }))
        .count();
    assert!(extra <= 1);
    fs::write(&p, "own\n").unwrap();
    assert_eq!(
        until(&rx, |e| if let VaultEvent::NoteChanged { text, .. } = e {
            Some(text)
        } else {
            None
        }),
        "own\n"
    );
    let temporary = d.path().join(".external-swap");
    fs::write(&temporary, "atomic save\n").unwrap();
    fs::rename(&temporary, &p).unwrap();
    assert_eq!(
        until(&rx, |e| if let VaultEvent::NoteChanged { text, .. } = e {
            Some(text)
        } else {
            None
        }),
        "atomic save\n"
    );
    let to = d.path().join("renamed.md");
    fs::rename(&p, &to).unwrap();
    let (from, new) = until(&rx, |e| {
        if let VaultEvent::NoteRenamed { from, to } = e {
            Some((from, to))
        } else {
            None
        }
    });
    assert_eq!(from, p);
    assert_eq!(new, to);
    fs::remove_file(&to).unwrap();
    until(&rx, |e| {
        matches!(e, VaultEvent::NoteRemoved { .. }).then_some(())
    });
}
#[test]
fn streamed_search_walk_cancel_and_match() {
    let d = tempfile::tempdir().unwrap();
    for n in 0..80 {
        fs::write(
            d.path().join(format!("n{n}.md")),
            "hello needle\nHello Other\n",
        )
        .unwrap();
    }
    let (h, rx) = open(d.path());
    let search = h.search(SearchSpec::parse("needle"));
    let hits = until(&rx, |e| {
        if let VaultEvent::Search(SearchEvent::Batch { hits, .. }) = e {
            Some(hits)
        } else {
            None
        }
    });
    assert!(!hits.is_empty());
    search.cancel();
    while rx.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(80));
    while let Ok(e) = rx.try_recv() {
        assert!(!matches!(e, VaultEvent::Search(_)));
    }
    let invalid = h.search(SearchSpec::parse("r:["));
    until(&rx, |e| {
        matches!(e, VaultEvent::Search(SearchEvent::Failed(..))).then_some(())
    });
    drop(invalid);
    let walk = h.walk_below(d.path());
    let mut paths = vec![];
    until(&rx, |e| match e {
        VaultEvent::Walk(WalkEvent::Batch { paths: p, .. }) => {
            paths.extend(p);
            None
        }
        VaultEvent::Walk(WalkEvent::Done(_)) => Some(()),
        _ => None,
    });
    assert_eq!(paths.len(), 80);
    drop(walk);
    let candidates = (0..3000).map(|n| nib_core::fuzzy::Candidate {
        label: format!("Note {n}"),
        text: format!("Note {n}"),
        target: nib_core::fuzzy::CandidateTarget::Note(id(&format!("n{n}"))),
    });
    let matcher = fuzzy::Matcher::new(candidates).unwrap();
    let query = matcher.set_query("Note 299");
    let events = matcher.events();
    let start = Instant::now();
    loop {
        while let Ok(s) = events.try_recv() {
            if s.query == query && !s.running {
                assert!(!s.candidates.is_empty());
                return;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
#[allow(clippy::too_many_lines)] // A complete user operation session, with assertions after each step.
fn operations_and_conflicts() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("dst")).unwrap();
    fs::write(d.path().join("a.md"), "original").unwrap();
    let (h, rx) = open(d.path());
    watch(&h);
    let src = d.path().join("a.md");
    let dest = d.path().join("dst");
    op(
        &h,
        &rx,
        FileOp::Copy {
            sources: vec![src.clone()],
            destination: dest.clone(),
        },
        None,
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("a.md")).unwrap(), b"original");
    let summary = op(
        &h,
        &rx,
        FileOp::Copy {
            sources: vec![src.clone()],
            destination: dest.clone(),
        },
        Some(ConflictAnswer::Skip),
    )
    .unwrap();
    assert_eq!(summary.files, 0);
    op(
        &h,
        &rx,
        FileOp::Copy {
            sources: vec![src.clone()],
            destination: dest.clone(),
        },
        Some(ConflictAnswer::Rename),
    )
    .unwrap();
    assert!(dest.join("a (1).md").exists());
    op(
        &h,
        &rx,
        FileOp::CreateFolder {
            path: d.path().join("folder"),
        },
        None,
    )
    .unwrap();
    op(
        &h,
        &rx,
        FileOp::CreateNote {
            id: id("created"),
            initial: "created".into(),
        },
        None,
    )
    .unwrap();
    op(
        &h,
        &rx,
        FileOp::Move {
            sources: vec![src.clone()],
            destination: d.path().join("folder"),
        },
        None,
    )
    .unwrap();
    assert!(!src.exists());
    let from = d.path().join("folder/a.md");
    let to = d.path().join("renamed.md");
    op(
        &h,
        &rx,
        FileOp::Rename {
            from,
            to: to.clone(),
        },
        None,
    )
    .unwrap();
    assert!(to.exists());
    assert!(
        op(
            &h,
            &rx,
            FileOp::Trash {
                paths: vec![to.clone()]
            },
            None
        )
        .is_err()
    );
    assert!(to.exists());
    assert!(
        op(
            &h,
            &rx,
            FileOp::CreateNote {
                id: id("created"),
                initial: "bad".into()
            },
            None
        )
        .is_err()
    );
    assert_eq!(fs::read(d.path().join("created.md")).unwrap(), b"created");
    assert!(h.write_attachment("../bad.png", b"x").is_err());
    let first_attachment = h.write_attachment("image.png", b"png").unwrap();
    let second_attachment = h.write_attachment("image.png", b"png2").unwrap();
    assert!(first_attachment.ends_with("image.png"));
    assert!(second_attachment.ends_with("image 1.png"));
    fs::write(d.path().join("latin1.md"), b"\xe9").unwrap();
    let invalid_note = h.read_note(&d.path().join("latin1.md")).unwrap();
    assert!(invalid_note.read_only);
    assert!(
        h.save(
            &d.path().join("latin1.md"),
            "would corrupt",
            &invalid_note.fmt
        )
        .is_err()
    );
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
    assert!(h.ensure_daily(date).unwrap().exists());
    assert_eq!(
        h.ensure_daily(date).unwrap(),
        d.path().join("daily/2026-10-16.md")
    );
}
#[test]
fn trash_and_replace_subprocess() {
    // GIO deliberately refuses trash on system-internal tmpfs mounts.
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(scratch).unwrap();
    let d = tempfile::Builder::new()
        .prefix("trash-")
        .tempdir_in(scratch)
        .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "subprocess_trash_entry"])
        .env("NIB_SUBPROCESS_ROOT", d.path())
        .env("XDG_DATA_HOME", d.path().join("data"))
        .env("NIB_ALLOW_ROOTS", d.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(d.path().join("data/Trash/files").is_dir());
}
#[test]
#[ignore = "invoked by trash_and_replace_subprocess with isolated XDG dirs"]
fn subprocess_trash_entry() {
    let root = std::path::PathBuf::from(std::env::var_os("NIB_SUBPROCESS_ROOT").unwrap());
    let vault = root.join("vault");
    fs::create_dir_all(vault.join("dst")).unwrap();
    fs::write(vault.join("a.md"), "new").unwrap();
    fs::write(vault.join("dst/a.md"), "old").unwrap();
    let (h, rx) =
        VaultHandle::open(&vault, &Config::default(), Guard::confined(vec![root])).unwrap();
    op(
        &h,
        &rx,
        FileOp::Copy {
            sources: vec![vault.join("a.md")],
            destination: vault.join("dst"),
        },
        Some(ConflictAnswer::Replace),
    )
    .unwrap();
    assert_eq!(fs::read(vault.join("dst/a.md")).unwrap(), b"new");
    op(
        &h,
        &rx,
        FileOp::Trash {
            paths: vec![vault.join("a.md")],
        },
        None,
    )
    .unwrap();
    assert!(!vault.join("a.md").exists());
}
#[test]
fn attachment_updates_and_empty_matcher() {
    let d = tempfile::tempdir().unwrap();
    let (h, rx) = open(d.path());
    watch(&h);
    fs::write(d.path().join("image.png"), b"test image").unwrap();
    until(&rx, |e| {
        matches!(e,VaultEvent::Index(IndexEvent::AttachmentUpsert(ref p))if p=="image.png")
            .then_some(())
    });
    fs::rename(d.path().join("image.png"), d.path().join("renamed.png")).unwrap();
    until(&rx, |e| {
        matches!(e,VaultEvent::Index(IndexEvent::AttachmentUpsert(ref p))if p=="renamed.png")
            .then_some(())
    });
    fs::remove_file(d.path().join("renamed.png")).unwrap();
    until(&rx, |e| {
        matches!(e,VaultEvent::Index(IndexEvent::AttachmentRemove(ref p))if p=="renamed.png")
            .then_some(())
    });
    let matcher = fuzzy::Matcher::new([]).unwrap();
    let query = matcher.set_query("missing");
    let events = matcher.events();
    let start = Instant::now();
    loop {
        if let Ok(s) = events.try_recv()
            && s.query == query
            && !s.running
        {
            assert_eq!(s.matched, 0);
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn deferred_start_reads_only_the_opened_note() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("a.md"), "# A").unwrap();
    let (h, rx) = VaultHandle::open_deferred(
        d.path(),
        &Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    assert!(!h.watcher_ready());
    assert_eq!(h.listing_reads(), 0);
    assert_eq!(h.read_note(&d.path().join("a.md")).unwrap().text, "# A");
    assert!(rx.try_recv().is_err());
    h.start_watching().unwrap();
    watch(&h);
    h.start_watching().unwrap();
}
