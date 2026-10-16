//! Shared temp-vault assertions, with deadlines on worker messages.
#![allow(dead_code)]
use nib_vault::{ops::FileOp, *};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub fn event<T>(
    rx: &async_channel::Receiver<VaultEvent>,
    mut f: impl FnMut(VaultEvent) -> Option<T>,
) -> T {
    let start = Instant::now();
    loop {
        while let Ok(e) = rx.try_recv() {
            if let Some(v) = f(e) {
                return v;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(10), "event deadline");
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub fn operation(
    h: &VaultHandle,
    rx: &async_channel::Receiver<VaultEvent>,
    op: FileOp,
) -> Result<OpSummary, String> {
    let token = h.op(op);
    event(rx, |e| match e {
        VaultEvent::Op(OpEvent::Done { id, summary }) if id == token => Some(Ok(summary)),
        VaultEvent::Op(OpEvent::Failed { id, message }) if id == token => Some(Err(message)),
        VaultEvent::Op(OpEvent::Conflict { id, .. }) if id == token => {
            panic!("unexpected conflict")
        }
        _ => None,
    })
}
pub fn index(h: &VaultHandle, rx: &async_channel::Receiver<VaultEvent>) -> Box<nib_core::Index> {
    h.start_index();
    event(rx, |e| {
        if let VaultEvent::Index(IndexEvent::Ready(i)) = e {
            Some(i)
        } else {
            None
        }
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub bytes: Vec<u8>,
    pub mode: u32,
    pub kind: u8,
}
pub fn manifest(root: &Path) -> BTreeMap<PathBuf, FileState> {
    fn walk(root: &Path, p: &Path, out: &mut BTreeMap<PathBuf, FileState>) {
        let m = p.symlink_metadata().unwrap();
        let rel = p.strip_prefix(root).unwrap().to_path_buf();
        let (kind, bytes) = if m.is_symlink() {
            (
                2,
                std::fs::read_link(p)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else if m.is_dir() {
            (1, vec![])
        } else {
            (0, std::fs::read(p).unwrap())
        };
        out.insert(
            rel,
            FileState {
                bytes,
                mode: m.permissions().mode() & 0o777,
                kind,
            },
        );
        if m.is_dir() {
            for e in std::fs::read_dir(p).unwrap() {
                walk(root, &e.unwrap().path(), out);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
pub fn fixture() -> tempfile::TempDir {
    let root = std::env::var_os("NIB_FIXTURE_ROOT")
        .map_or_else(|| glib::user_cache_dir().join("nib-fixture"), PathBuf::from);
    assert!(
        root.join(".fixture-marker").is_file(),
        "run scripts/mkfixture.sh before integration tests"
    );
    let d = tempfile::tempdir().unwrap();
    let guard = Guard::confined(vec![d.path().into()]);
    for e in std::fs::read_dir(root).unwrap() {
        let e = e.unwrap();
        fsx::copy(&guard, &e.path(), &d.path().join(e.file_name())).unwrap();
    }
    d
}
