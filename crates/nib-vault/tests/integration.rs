//! Full-fixture round trips, rename integration and no-pollution proof.
mod support;
use nib_core::{NoteId, settings::Config};
use nib_vault::{ops::FileOp, *};
use std::{collections::BTreeSet, fs, path::PathBuf};
fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}
#[test]
#[allow(clippy::too_many_lines)] // A scripted user session compared against a full before/after manifest.
fn fixture_roundtrip_rename_and_no_pollution() {
    let d = support::fixture();
    let before = support::manifest(d.path());
    let (h, rx) = VaultHandle::open(
        d.path(),
        &Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    for name in [
        "crlf.md",
        "bom.md",
        "mixed-eol.md",
        "no-trailing-newline.md",
        "linked-symlink.md",
    ] {
        let path = d.path().join(name);
        let bytes = fs::read(&path).unwrap();
        let note = h.read_note(&path).unwrap();
        assert!(!note.read_only);
        h.save(&path, &note.text, &note.fmt).unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes, "{name}");
    }
    let invalid = d.path().join("latin1.md");
    let note = h.read_note(&invalid).unwrap();
    assert!(note.read_only);
    assert!(h.save(&invalid, &note.text, &note.fmt).is_err());
    let index = support::index(&h, &rx);
    assert_eq!(index.backlinks(&id("hub")).len(), 14);
    assert!(index.notes.contains_key(&id("latin1")));
    let plan = nib_core::rename_plan(&index, &id("hub"), &id("hub-renamed"));
    assert_eq!(plan.len(), 14);
    support::operation(
        &h,
        &rx,
        FileOp::Rename {
            from: d.path().join("hub.md"),
            to: d.path().join("hub-renamed.md"),
        },
    )
    .unwrap();
    let summary = support::operation(&h, &rx, FileOp::ApplyEdits(plan.clone())).unwrap();
    assert_eq!(summary.links, 14);
    assert_eq!(summary.files, 14);
    assert_eq!(summary.edits, plan);
    let index = support::index(&h, &rx);
    assert_eq!(index.backlinks(&id("hub-renamed")).len(), 14);
    for n in 0..14 {
        let text = fs::read_to_string(d.path().join(format!("linker{n:02}.md"))).unwrap();
        assert!(text.contains(&format!("|alias {n}]]")));
        assert!(!text.contains("[[hub#"));
        assert!(index.notes[&id(&format!("linker{n:02}"))].links.iter().all(|l|matches!(&l.target,nib_core::LinkTarget::Note{id:target,..}if *target==id("hub-renamed"))));
    }
    h.create_note(&id("created"), "# Created\n").unwrap();
    h.write_attachment("session.png", b"fixture attachment")
        .unwrap();
    support::operation(
        &h,
        &rx,
        FileOp::Copy {
            sources: vec![d.path().join("created.md")],
            destination: d.path().join("folder00"),
        },
    )
    .unwrap();
    support::operation(
        &h,
        &rx,
        FileOp::Move {
            sources: vec![d.path().join("folder00/created.md")],
            destination: d.path().join("folder01"),
        },
    )
    .unwrap();
    assert!(h.create_note(&id(".obsidian/evil"), "forbidden").is_err());
    assert!(
        support::operation(
            &h,
            &rx,
            FileOp::Trash {
                paths: vec![d.path().join("created.md")]
            }
        )
        .is_err()
    );
    assert!(d.path().join("created.md").exists());
    drop(h);
    let after = support::manifest(d.path());
    let mut changed = BTreeSet::new();
    for p in before.keys().chain(after.keys()) {
        if before.get(p) != after.get(p) {
            changed.insert(p.clone());
        }
    }
    let mut expected: BTreeSet<PathBuf> = [
        "hub.md",
        "hub-renamed.md",
        "created.md",
        "folder01/created.md",
        "attachments/session.png",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    for n in 0..14 {
        expected.insert(format!("linker{n:02}.md").into());
    }
    assert_eq!(changed, expected);
    for (p, state) in &before {
        if p.components()
            .any(|c| c.as_os_str() == ".obsidian" || c.as_os_str() == ".trash")
        {
            assert_eq!(after.get(p), Some(state));
        }
    }
    assert!(
        !after
            .keys()
            .any(|p| p.to_string_lossy().ends_with(".nib-tmp"))
    );
}
#[test]
fn every_mutation_is_guarded() {
    let d = tempfile::tempdir().unwrap();
    let allowed = d.path().join("allowed");
    fs::create_dir(&allowed).unwrap();
    fs::create_dir(d.path().join("denied")).unwrap();
    fs::write(allowed.join("a.md"), "allowed").unwrap();
    let denied = d.path().join("denied/a.md");
    fs::write(&denied, "denied").unwrap();
    let guard = Guard::confined(vec![allowed.clone()]);
    let (h, rx) = VaultHandle::open(d.path(), &Config::default(), guard).unwrap();
    let before = support::manifest(d.path());
    let ops = vec![
        FileOp::Copy {
            sources: vec![allowed.join("a.md")],
            destination: d.path().join("denied"),
        },
        FileOp::Move {
            sources: vec![allowed.join("a.md")],
            destination: d.path().join("denied"),
        },
        FileOp::Rename {
            from: allowed.join("a.md"),
            to: d.path().join("denied/new.md"),
        },
        FileOp::Trash {
            paths: vec![denied.clone()],
        },
        FileOp::CreateNote {
            id: id("denied/new"),
            initial: "bad".into(),
        },
        FileOp::CreateFolder {
            path: d.path().join("denied/new"),
        },
        FileOp::ApplyEdits(vec![nib_core::Edit {
            note: id("denied/a"),
            byte_range: 0..1,
            replacement: "bad".into(),
            expected: "d".into(),
            source_hash: 0,
        }]),
        FileOp::CopyAttachment {
            source: allowed.join("a.md"),
        },
    ];
    for op in ops {
        assert!(support::operation(&h, &rx, op).is_err());
    }
    assert!(
        h.save(&denied, "bad", &nib_core::text::TextFormat::default())
            .is_err()
    );
    assert!(h.create_note(&id("denied/new"), "bad").is_err());
    assert!(h.write_attachment("bad.png", b"bad").is_err());
    assert!(h.checked_path(&d.path().join("../outside")).is_err());
    assert_eq!(support::manifest(d.path()), before);
}
#[test]
fn stale_rename_plan_never_replaces_unrelated_text() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("hub.md"), "# Hub").unwrap();
    fs::write(d.path().join("source.md"), "[[hub]]").unwrap();
    let (h, rx) = VaultHandle::open(
        d.path(),
        &Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    let i = support::index(&h, &rx);
    let plan = nib_core::rename_plan(&i, &id("hub"), &id("renamed"));
    for text in ["[[new]]", "  hub [[other]]"] {
        fs::write(d.path().join("source.md"), text).unwrap();
        assert!(support::operation(&h, &rx, FileOp::ApplyEdits(plan.clone())).is_err());
        assert_eq!(
            fs::read_to_string(d.path().join("source.md")).unwrap(),
            text
        );
    }
}
#[test]
fn folder_rename_updates_open_paths_without_waiting_for_a_watcher() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("old")).unwrap();
    fs::write(d.path().join("old/a.md"), "[other](b.md) [[old/b|alias]]").unwrap();
    fs::write(d.path().join("old/b.md"), "# B").unwrap();
    let (h, rx) = VaultHandle::open_deferred(
        d.path(),
        &Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    let i = support::index(&h, &rx);
    let plan = nib_core::rename_plan(&i, &id("old"), &id("new"));
    let token = h.op(FileOp::Rename {
        from: d.path().join("old"),
        to: d.path().join("new"),
    });
    let mut renamed = Vec::new();
    let mut parsed = BTreeSet::new();
    support::event(&rx, |e| match e {
        VaultEvent::NoteRenamed { from, to } => {
            renamed.push((from, to));
            None
        }
        VaultEvent::Index(IndexEvent::Upsert(meta)) => {
            parsed.insert(meta.id.clone());
            None
        }
        VaultEvent::Op(OpEvent::Done { id, .. }) if id == token => Some(()),
        VaultEvent::Op(OpEvent::Failed { message, .. }) => panic!("{message}"),
        _ => None,
    });
    assert_eq!(renamed.len(), 2);
    assert_eq!(parsed, [id("new/a"), id("new/b")].into_iter().collect());
    support::operation(&h, &rx, FileOp::ApplyEdits(plan)).unwrap();
    let i = support::index(&h, &rx);
    assert!(!i.notes.contains_key(&id("old/a")));
    assert!(i.notes[&id("new/a")].links.iter().all(
        |l| matches!(&l.target,nib_core::LinkTarget::Note{id:target,..}if *target==id("new/b"))
    ));
}
