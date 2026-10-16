//! Guarded file operations, conflict questions and atomic rename-plan rewrites.
use crate::{
    VaultError,
    events::{ConflictAnswer, IndexEvent, OpEvent, OpId, OpSummary, VaultEvent},
    fsx,
    service::{Inner, hash, lock},
};
use nib_core::{Edit, NoteId};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};
/// No permanent-delete variant exists: every removal goes through XDG trash.
#[derive(Debug, Clone)]
pub enum FileOp {
    /// Copy entries into a destination folder.
    Copy {
        /// Absolute sources.
        sources: Vec<PathBuf>,
        /// Destination folder.
        destination: PathBuf,
    },
    /// Move entries into a destination folder.
    Move {
        /// Absolute sources.
        sources: Vec<PathBuf>,
        /// Destination folder.
        destination: PathBuf,
    },
    /// Trash guarded source entries, with no deletion fallback.
    Trash {
        /// Absolute entries.
        paths: Vec<PathBuf>,
    },
    /// Rename a note, file or folder (link rewrites are a separate approved op).
    Rename {
        /// Absolute old path.
        from: PathBuf,
        /// Absolute new path.
        to: PathBuf,
    },
    /// Create a new UTF-8 note, refusing overwrite.
    CreateNote {
        /// Relative identity.
        id: NoteId,
        /// Optional initial/template text.
        initial: String,
    },
    /// Create a new folder, refusing an existing entry.
    CreateFolder {
        /// Absolute destination.
        path: PathBuf,
    },
    /// Apply an approved rename plan, highest offsets first.
    ApplyEdits(Vec<Edit>),
    /// Copy a user-selected external attachment into the configured folder.
    CopyAttachment {
        /// Absolute source; only the destination is mutated.
        source: PathBuf,
    },
}
fn free_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 1.. {
        let candidate = path.with_file_name(format!("{stem} ({i}){ext}"));
        if candidate.symlink_metadata().is_err() {
            return candidate;
        }
    }
    unreachable!("an unbounded suffix space has a free filename")
}
fn destination(
    inner: &Inner,
    id: OpId,
    path: PathBuf,
    answer: &mut Option<ConflictAnswer>,
    rx: &async_channel::Receiver<ConflictAnswer>,
) -> Result<Option<PathBuf>, VaultError> {
    if path.symlink_metadata().is_err() {
        return Ok(Some(path));
    }
    if answer.is_none() {
        inner.emit(VaultEvent::Op(OpEvent::Conflict {
            id,
            path: path.clone(),
        }));
        *answer = Some(
            rx.recv_blocking()
                .map_err(|_| VaultError::Message("operation cancelled".into()))?,
        );
    }
    match answer {
        Some(ConflictAnswer::Skip) => Ok(None),
        Some(ConflictAnswer::Rename) => Ok(Some(free_name(&path))),
        Some(ConflictAnswer::Replace) => {
            fsx::trash(&inner.guard, &path)?;
            Ok(Some(path))
        }
        None => Err(VaultError::Message("missing conflict answer".into())),
    }
}
fn checked(inner: &Inner, path: &Path) -> Result<PathBuf, VaultError> {
    let p = inner.path(path)?;
    if p == inner.root {
        return Err(VaultError::Denied(path.into()));
    }
    inner.guard.check(&p)?;
    Ok(p)
}
fn check_tree(inner: &Inner, source: &Path, dest: Option<&Path>) -> Result<(), VaultError> {
    checked(inner, source)?;
    if let Some(dest) = dest {
        checked(inner, dest)?;
    }
    let meta = std::fs::symlink_metadata(source)?;
    if meta.is_dir() {
        for e in std::fs::read_dir(source)? {
            let e = e?;
            let target = dest.map(|p| p.join(e.file_name()));
            check_tree(inner, &e.path(), target.as_deref())?;
        }
    }
    Ok(())
}
fn preflight(inner: &Inner, op: &FileOp) -> Result<(), VaultError> {
    match op {
        FileOp::Copy {
            sources,
            destination,
        }
        | FileOp::Move {
            sources,
            destination,
        } => {
            let dest = inner.path(destination)?;
            inner.guard.check(&dest)?;
            for src in sources {
                let source = checked(inner, src)?;
                let to = dest.join(
                    src.file_name()
                        .ok_or_else(|| VaultError::Denied(src.clone()))?,
                );
                checked(inner, &to)?;
                check_tree(inner, src, Some(&to))?;
                if to == source || dest.starts_with(&source) {
                    return Err(VaultError::Denied(to));
                }
            }
        }
        FileOp::Trash { paths } => {
            for p in paths {
                check_tree(inner, p, None)?;
            }
        }
        FileOp::Rename { from, to } => {
            let source = checked(inner, from)?;
            let target = checked(inner, to)?;
            check_tree(inner, from, Some(to))?;
            if target.starts_with(&source) {
                return Err(VaultError::Denied(to.clone()));
            }
        }
        FileOp::CreateNote { id, .. } => {
            checked(inner, &inner.root.join(id.filename()))?;
        }
        FileOp::CreateFolder { path } => {
            checked(inner, path)?;
        }
        FileOp::ApplyEdits(edits) => {
            for e in edits {
                checked(inner, &inner.root.join(e.note.filename()))?;
            }
        }
        FileOp::CopyAttachment { source } => {
            if !source.is_absolute() || !source.is_file() {
                return Err(VaultError::Denied(source.clone()));
            }
            let dest = inner.path(&inner.root.join(&inner.cfg.vault.attachments))?;
            inner.guard.check(&dest)?;
            let name = source
                .file_name()
                .ok_or_else(|| VaultError::Denied(source.clone()))?;
            checked(inner, &dest.join(name))?;
        }
    }
    Ok(())
}
fn notify_move(inner: &Inner, from: &Path, to: &Path) {
    if from
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        if let (Ok(a), Ok(b)) = (inner.id(from), inner.id(to)) {
            inner.emit_index(IndexEvent::Renamed(a, b));
        }
        inner.emit(VaultEvent::NoteRenamed {
            from: from.into(),
            to: to.into(),
        });
    }
    if let Some(p) = from.parent() {
        inner.invalidate(p);
    }
    if let Some(p) = to.parent() {
        inner.invalidate(p);
    }
}
#[allow(clippy::too_many_lines)] // A single exhaustive operation dispatch owns progress and conflict state.
fn execute(
    inner: &Inner,
    id: OpId,
    op: &FileOp,
    rx: &async_channel::Receiver<ConflictAnswer>,
) -> Result<OpSummary, VaultError> {
    preflight(inner, op)?;
    let mut summary = OpSummary::default();
    let mut answer = None;
    match op {
        FileOp::Copy {
            sources,
            destination: folder,
        }
        | FileOp::Move {
            sources,
            destination: folder,
        } => {
            for (n, src) in sources.iter().enumerate() {
                if inner.stopped.load(Ordering::Acquire) {
                    return Err(VaultError::Message("operation cancelled".into()));
                }
                let path = folder.join(
                    src.file_name()
                        .ok_or_else(|| VaultError::Denied(src.clone()))?,
                );
                let Some(path) = destination(inner, id, path, &mut answer, rx)? else {
                    continue;
                };
                if matches!(op, FileOp::Move { .. }) {
                    fsx::move_entry(&inner.guard, src, &path)?;
                    notify_move(inner, src, &path);
                } else {
                    fsx::copy(&inner.guard, src, &path)?;
                    inner.invalidate(folder);
                }
                summary.files += 1;
                inner.emit(VaultEvent::Op(OpEvent::Progress {
                    id,
                    done: n + 1,
                    total: sources.len(),
                }));
            }
        }
        FileOp::Rename { from, to } => {
            if let Some(to) = destination(inner, id, to.clone(), &mut answer, rx)? {
                fsx::move_entry(&inner.guard, from, &to)?;
                notify_move(inner, from, &to);
                summary.files = 1;
            }
        }
        FileOp::Trash { paths } => {
            for (n, path) in paths.iter().enumerate() {
                fsx::trash(&inner.guard, path)?;
                if let Ok(id) = inner.id(path) {
                    inner.emit_index(IndexEvent::Remove(id));
                }
                inner.emit(VaultEvent::NoteRemoved { path: path.clone() });
                if let Some(p) = path.parent() {
                    inner.invalidate(p);
                }
                summary.files += 1;
                inner.emit(VaultEvent::Op(OpEvent::Progress {
                    id,
                    done: n + 1,
                    total: paths.len(),
                }));
            }
        }
        FileOp::CreateNote { id, initial } => {
            let path = inner.path(&inner.root.join(id.filename()))?;
            if let Some(p) = path.parent() {
                fsx::create_dir_all(&inner.guard, p)?;
            }
            fsx::create(&inner.guard, &path, initial.as_bytes())?;
            lock(&inner.writes).insert(path.clone(), (0, hash(initial.as_bytes())));
            inner.changed(&path, initial.as_bytes(), true);
            summary.files = 1;
        }
        FileOp::CreateFolder { path } => {
            if path.symlink_metadata().is_ok() {
                return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists).into());
            }
            fsx::create_dir_all(&inner.guard, path)?;
            if let Some(p) = path.parent() {
                inner.invalidate(p);
            }
            summary.files = 1;
        }
        FileOp::ApplyEdits(edits) => {
            let mut files = BTreeMap::<NoteId, Vec<&Edit>>::new();
            for e in edits {
                files.entry(e.note.clone()).or_default().push(e);
            }
            let mut prepared = Vec::new();
            for (note, mut edits) in files {
                let path = inner.path(&inner.root.join(note.filename()))?;
                let bytes = std::fs::read(&path)?;
                let (mut text, fmt) = nib_core::text::decode(&bytes)?;
                edits.sort_by_key(|e| std::cmp::Reverse(e.byte_range.start));
                let mut previous = text.len();
                for e in &edits {
                    if e.byte_range.end > previous
                        || e.byte_range.start > e.byte_range.end
                        || !text.is_char_boundary(e.byte_range.start)
                        || !text.is_char_boundary(e.byte_range.end)
                    {
                        return Err(VaultError::Message(
                            "rename plan has stale or overlapping offsets".into(),
                        ));
                    }
                    text.replace_range(e.byte_range.clone(), &e.replacement);
                    previous = e.byte_range.start;
                }
                prepared.push((path, nib_core::text::encode(&text, &fmt)));
            }
            for (path, bytes) in prepared {
                let mut writes = lock(&inner.writes);
                let g = fsx::save(&inner.guard, &path, &bytes)?;
                writes.insert(path.clone(), (g, hash(&bytes)));
                drop(writes);
                inner.changed(&path, &bytes, true);
                summary.files += 1;
            }
            summary.links = edits.len();
            summary.edits.clone_from(edits);
        }
        FileOp::CopyAttachment { source } => {
            let name = source
                .file_name()
                .ok_or_else(|| VaultError::Denied(source.clone()))?;
            let folder = inner.path(&inner.root.join(&inner.cfg.vault.attachments))?;
            fsx::create_dir_all(&inner.guard, &folder)?;
            if let Some(to) = destination(inner, id, folder.join(name), &mut answer, rx)? {
                fsx::copy(&inner.guard, source, &to)?;
                inner.invalidate(&folder);
                summary.files = 1;
            }
        }
    }
    Ok(summary)
}
pub(crate) fn start(inner: &Arc<Inner>, op: FileOp) -> OpId {
    let id = inner.request_id();
    let (tx, rx) = async_channel::bounded(1);
    lock(&inner.pending).insert(id, tx);
    let weak = Arc::downgrade(inner);
    if let Err(e) = std::thread::Builder::new()
        .name("nib-op".into())
        .spawn(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let result = execute(&inner, id, &op, &rx);
            lock(&inner.pending).remove(&id);
            inner.emit(VaultEvent::Op(match result {
                Ok(summary) => OpEvent::Done { id, summary },
                Err(e) => OpEvent::Failed {
                    id,
                    message: e.to_string(),
                },
            }));
        })
    {
        lock(&inner.pending).remove(&id);
        inner.notice(e.to_string());
    }
    id
}
pub(crate) fn write_attachment(
    inner: &Inner,
    name: &str,
    bytes: &[u8],
) -> Result<PathBuf, VaultError> {
    if name.is_empty() || Path::new(name).file_name() != Some(std::ffi::OsStr::new(name)) {
        return Err(VaultError::Denied(name.into()));
    }
    let folder = inner.path(&inner.root.join(&inner.cfg.vault.attachments))?;
    inner.guard.check(&folder)?;
    fsx::create_dir_all(&inner.guard, &folder)?;
    for n in 0.. {
        let p = folder.join(nib_core::attach::collision_name(name, n));
        match fsx::create(&inner.guard, &p, bytes) {
            Ok(()) => {
                inner.invalidate(&folder);
                return Ok(p);
            }
            Err(VaultError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    unreachable!("unbounded collision suffixes")
}
