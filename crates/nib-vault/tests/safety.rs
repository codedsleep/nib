//! Guard, atomic-save, session and format contracts, without a display.
use nib_vault::{Guard, fsx, state};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    time::{Duration, SystemTime},
};
#[test]
fn guard_resolves_escapes() {
    let d = tempfile::tempdir().unwrap();
    let allowed = d.path().join("allowed");
    let other = d.path().join("other");
    fs::create_dir(&allowed).unwrap();
    fs::create_dir(&other).unwrap();
    let g = Guard::confined(vec![allowed.clone()]);
    assert!(g.check(&allowed.join("new/n.md")).is_ok());
    assert!(g.check(&allowed.join("../other/n.md")).is_err());
    symlink(&other, allowed.join("escape")).unwrap();
    assert!(g.check(&allowed.join("escape/n.md")).is_err());
    assert!(fsx::save(&g, &other.join("n.md"), b"bad").is_err());
}
#[test]
fn atomic_symlink_and_permissions() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("note.md");
    let link = d.path().join("link.md");
    fs::write(&file, b"old").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    symlink("note.md", &link).unwrap();
    let g = Guard::confined(vec![d.path().into()]);
    fsx::save(&g, &link, b"new").unwrap();
    assert!(link.symlink_metadata().unwrap().is_symlink());
    assert_eq!(fs::read(file).unwrap(), b"new");
    assert_eq!(
        fs::metadata(link).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
#[test]
fn failed_save_keeps_original() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("note.md");
    fs::write(&p, b"old").unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let g = Guard::confined(vec![d.path().into()]);
    let result = fsx::save(&g, &p, b"new");
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(p).unwrap(), b"old");
}
#[test]
fn stale_cleanup_never_follows_symlinks() {
    let d = tempfile::tempdir().unwrap();
    let old = d.path().join(".old.nib-tmp");
    let fresh = d.path().join(".fresh.nib-tmp");
    fs::write(&old, b"old").unwrap();
    fs::write(&fresh, b"fresh").unwrap();
    let file = fs::File::options().write(true).open(&old).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(120))
        .unwrap();
    symlink(".old.nib-tmp", d.path().join(".link.nib-tmp")).unwrap();
    let g = Guard::confined(vec![d.path().into()]);
    assert_eq!(
        fsx::cleanup_stale_tmp(&g, d.path(), SystemTime::now()).unwrap(),
        vec![old]
    );
    assert!(fresh.exists());
    assert!(
        d.path()
            .join(".link.nib-tmp")
            .symlink_metadata()
            .unwrap()
            .is_symlink()
    );
}
#[test]
fn state_roundtrip_and_roots() {
    let d = tempfile::tempdir().unwrap();
    let p = state::Paths {
        config: d.path().join("config"),
        state: d.path().join("state"),
    };
    let g = Guard::confined(vec![d.path().into()]);
    assert_eq!(
        state::load_config(&p, &g).unwrap().0,
        nib_core::settings::Config::default()
    );
    let s = state::Session::default();
    let file = p.state.join("vault/session.toml");
    s.save(&file, &g).unwrap();
    assert_eq!(state::Session::load(&file), (s, vec![]));
    fs::write(&file, "bad = [").unwrap();
    assert!(!state::Session::load(&file).1.is_empty());
    state::append_crash(&p, &g, "test\n").unwrap();
    assert_eq!(
        fs::read_to_string(p.state.join("crash.log")).unwrap(),
        "test\n"
    );
    fs::create_dir(d.path().join(".obsidian")).unwrap();
    fs::create_dir(d.path().join("folder")).unwrap();
    let note = d.path().join("folder/n.md");
    assert_eq!(state::find_vault_root(None, &note).unwrap(), d.path());
    assert_eq!(
        state::find_vault_root(Some(&d.path().join("folder")), &note).unwrap(),
        d.path().join("folder")
    );
    assert_eq!(
        state::vault_key(Path::new("/tmp")).unwrap(),
        "e9671acd244849c5"
    );
}
#[test]
fn opening_a_nested_note_recovers_its_stale_temp() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("nested")).unwrap();
    let path = d.path().join("nested/n.md");
    let tmp = d.path().join("nested/.n.nib-tmp");
    fs::write(&path, "old").unwrap();
    fs::write(&tmp, "partial").unwrap();
    fs::File::options()
        .write(true)
        .open(&tmp)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(120))
        .unwrap();
    let (h, _) = nib_vault::VaultHandle::open_deferred(
        d.path(),
        &nib_core::settings::Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    assert_eq!(h.read_note(&path).unwrap().text, "old");
    assert!(!tmp.exists());
}
#[test]
fn a_directory_is_never_opened_or_saved_as_a_note() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    fs::create_dir(&root).unwrap();
    let sibling = d.path().join(".vault.nib-tmp");
    fs::write(&sibling, "not this vault's temporary").unwrap();
    fs::File::options()
        .write(true)
        .open(&sibling)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(120))
        .unwrap();
    let (h, _) = nib_vault::VaultHandle::open_deferred(
        &root,
        &nib_core::settings::Config::default(),
        Guard::confined(vec![d.path().into()]),
    )
    .unwrap();
    assert!(h.read_note(&root).is_err());
    assert!(
        h.save(&root, "bad", &nib_core::text::TextFormat::default())
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(sibling).unwrap(),
        "not this vault's temporary"
    );
}
