//! Vault lifetime, startup fast path, note I/O and self-write tracking.
use crate::{
    Generation, Guard, VaultError,
    events::{ConflictAnswer, IndexEvent, Notice, OpId, VaultEvent},
    fsx,
    listing::{ListRequest, ListingCache},
};
use async_channel::{Receiver, Sender};
use nib_core::{
    NoteId,
    settings::Config,
    text::{self, TextFormat},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::SystemTime,
};
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
pub(crate) fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
pub(crate) struct Inner {
    pub root: PathBuf,
    pub cfg: Config,
    pub guard: Guard,
    pub tx: Sender<VaultEvent>,
    pub list_tx: Sender<ListRequest>,
    pub cache: Mutex<ListingCache>,
    pub hashes: Mutex<HashMap<PathBuf, u64>>,
    pub writes: Mutex<HashMap<PathBuf, (Generation, u64)>>,
    pub pending: Mutex<HashMap<OpId, Sender<ConflictAnswer>>>,
    pub stopped: AtomicBool,
    pub handles: AtomicUsize,
    pub next: AtomicU64,
    pub indexing: AtomicBool,
    pub index_updates: Mutex<Vec<IndexEvent>>,
    pub watch_ready: AtomicBool,
    pub fallback: AtomicBool,
    pub watcher: Mutex<Option<crate::watch::Watcher>>,
}
impl Inner {
    pub fn emit(&self, event: VaultEvent) {
        if !self.stopped.load(Ordering::Acquire) {
            let _ = self.tx.try_send(event);
        }
    }
    pub fn emit_index(&self, event: IndexEvent) {
        let mut updates = lock(&self.index_updates);
        if self.indexing.load(Ordering::Acquire) {
            updates.push(event);
        } else {
            self.emit(VaultEvent::Index(event));
        }
    }
    pub fn notice(&self, message: impl Into<String>) {
        self.emit(VaultEvent::Notice(Notice {
            message: message.into(),
            relist_on_focus: self.fallback.load(Ordering::Acquire),
        }));
    }
    pub fn request_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
    pub fn path(&self, path: &Path) -> Result<PathBuf, VaultError> {
        let resolved = crate::guard::resolve(path)?;
        let relative = resolved
            .strip_prefix(&self.root)
            .map_err(|_| VaultError::Denied(path.into()))?;
        if relative.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(nib_core::sort::never_shown)
        }) {
            return Err(VaultError::Denied(path.into()));
        }
        Ok(resolved)
    }
    pub fn id(&self, path: &Path) -> Result<NoteId, VaultError> {
        let rel = path
            .strip_prefix(&self.root)
            .map_err(|_| VaultError::Denied(path.into()))?;
        NoteId::new(
            rel.to_str()
                .ok_or_else(|| VaultError::Denied(path.into()))?,
        )
        .map_err(Into::into)
    }
    pub fn invalidate(&self, folder: &Path) {
        lock(&self.cache).invalidate(folder);
        let _ = self.list_tx.try_send(ListRequest {
            folder: folder.into(),
            force: true,
            only_cached: true,
        });
    }
    pub fn changed(&self, path: &Path, bytes: &[u8], own: bool) {
        let digest = hash(bytes);
        lock(&self.hashes).insert(path.into(), digest);
        if let Ok((text, fmt)) = text::decode(bytes) {
            if let (Ok(id), Ok(m)) = (self.id(path), std::fs::metadata(path)) {
                self.emit_index(IndexEvent::Upsert(Box::new(nib_core::parse_note(
                    id,
                    &text,
                    m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    m.len(),
                ))));
            }
            if !own {
                self.emit(VaultEvent::NoteChanged {
                    path: path.into(),
                    text,
                    fmt,
                });
            }
        } else if !own {
            self.emit(VaultEvent::NoteInvalid { path: path.into() });
        }
        if let Some(p) = path.parent() {
            self.invalidate(p);
        }
    }
}
/// A decoded note, or a read-only lossy preview of non-UTF-8 bytes.
#[derive(Debug, Clone)]
pub struct NoteText {
    /// Text to display.
    pub text: String,
    /// Remembered encoding format.
    pub fmt: TextFormat,
    /// Refuse all saves while this flag is set.
    pub read_only: bool,
    /// Optional explanation for a read-only preview.
    pub notice: Option<String>,
}
/// Cloneable vault service handle. Dropping the final handle cancels workers,
/// closes pending conflict questions and releases watches.
pub struct VaultHandle {
    pub(crate) inner: Arc<Inner>,
}
impl Clone for VaultHandle {
    fn clone(&self) -> Self {
        self.inner.handles.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl Drop for VaultHandle {
    fn drop(&mut self) {
        if self.inner.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inner.stopped.store(true, Ordering::Release);
            self.inner.list_tx.close();
            self.inner.tx.close();
            lock(&self.inner.pending).clear();
            let watch = lock(&self.inner.watcher).take();
            drop(watch);
        }
    }
}
impl VaultHandle {
    /// Canonicalize only the root and start background services without walking here.
    pub fn open(
        root: &Path,
        cfg: &Config,
        guard: Guard,
    ) -> Result<(Self, Receiver<VaultEvent>), VaultError> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(VaultError::Denied(root));
        }
        let (tx, rx) = async_channel::unbounded();
        let (list_tx, list_rx) = async_channel::unbounded();
        let inner = Arc::new(Inner {
            root,
            cfg: cfg.clone(),
            guard,
            tx,
            list_tx,
            cache: Mutex::new(ListingCache::default()),
            hashes: Mutex::new(HashMap::new()),
            writes: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            stopped: AtomicBool::new(false),
            handles: AtomicUsize::new(1),
            next: AtomicU64::new(1),
            indexing: AtomicBool::new(false),
            index_updates: Mutex::new(Vec::new()),
            watch_ready: AtomicBool::new(false),
            fallback: AtomicBool::new(false),
            watcher: Mutex::new(None),
        });
        crate::listing::start(&inner, list_rx)?;
        crate::watch::start(&inner)?;
        Ok((Self { inner }, rx))
    }
    /// Absolute canonical vault root.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }
    /// Validate an absolute path for image decoding or launching in the UI.
    /// Symlink targets and protected vault directories obey the same read policy.
    pub fn checked_path(&self, path: &Path) -> Result<PathBuf, VaultError> {
        self.inner.path(path)
    }
    /// Whether the recursive watcher has finished registration.
    pub fn watcher_ready(&self) -> bool {
        self.inner.watch_ready.load(Ordering::Acquire)
    }
    /// Whether focus-in should force a listing refresh.
    pub fn needs_relist(&self) -> bool {
        self.inner.fallback.load(Ordering::Acquire)
    }
    /// Read one contained note synchronously for the first editable buffer.
    pub fn read_note(&self, abs: &Path) -> Result<NoteText, VaultError> {
        let path = self.inner.path(abs)?;
        let bytes = std::fs::read(&path)?;
        lock(&self.inner.hashes).insert(path, hash(&bytes));
        match text::decode(&bytes) {
            Ok((text, fmt)) => Ok(NoteText {
                text,
                fmt,
                read_only: false,
                notice: None,
            }),
            Err(_) => Ok(NoteText {
                text: String::from_utf8_lossy(&bytes).into_owned(),
                fmt: TextFormat::default(),
                read_only: true,
                notice: Some("not UTF-8; opened read-only".into()),
            }),
        }
    }
    /// Atomically save an editable UTF-8 note and emit an index patch.
    pub fn save(&self, abs: &Path, text: &str, fmt: &TextFormat) -> Result<Generation, VaultError> {
        let path = self.inner.path(abs)?;
        if let Ok(existing) = std::fs::read(&path) {
            text::decode(&existing)?;
        }
        let bytes = text::encode(text, fmt);
        let mut writes = lock(&self.inner.writes);
        let generation = fsx::save(&self.inner.guard, &path, &bytes)?;
        writes.insert(path.clone(), (generation, hash(&bytes)));
        drop(writes);
        self.inner.changed(&path, &bytes, true);
        Ok(generation)
    }
    /// Request a cached listing; visibility preferences are applied by core/UI.
    pub fn list(&self, folder: &Path) {
        self.request_list(folder, false);
    }
    /// Force a disk read, e.g. F5 or watcher fallback.
    pub fn reread(&self, folder: &Path) {
        self.request_list(folder, true);
    }
    fn request_list(&self, folder: &Path, force: bool) {
        match self.inner.path(folder) {
            Ok(folder) => {
                let _ = self.inner.list_tx.try_send(ListRequest {
                    folder,
                    force,
                    only_cached: false,
                });
            }
            Err(e) => self.inner.notice(e.to_string()),
        }
    }
    /// Number of folder disk reads, for trace/tests of read-once behavior.
    pub fn listing_reads(&self) -> usize {
        lock(&self.inner.cache).reads
    }
    /// Start a background parallel parse; repeated calls during a build coalesce.
    pub fn start_index(&self) {
        crate::indexer::start(&self.inner);
    }
    /// Search at ripgrep speed; dropping the handle cancels further emissions.
    pub fn search(&self, spec: nib_core::search::SearchSpec) -> crate::search::SearchHandle {
        crate::search::start(&self.inner, spec)
    }
    /// Stream all visible paths below a folder.
    pub fn walk_below(&self, folder: &Path) -> crate::search::WalkHandle {
        crate::search::walk(&self.inner, folder)
    }
    /// Dispatch one guarded file operation on a blocking worker.
    pub fn op(&self, op: crate::ops::FileOp) -> OpId {
        crate::ops::start(&self.inner, op)
    }
    /// Answer a single operation's conflict; the answer applies to later conflicts too.
    pub fn answer(&self, op: OpId, answer: ConflictAnswer) {
        if let Some(tx) = lock(&self.inner.pending).get(&op) {
            let _ = tx.try_send(answer);
        }
    }
    /// Create a note without overwriting anything, including a dangling symlink.
    pub fn create_note(&self, id: &NoteId, initial: &str) -> Result<PathBuf, VaultError> {
        let path = self.inner.path(&self.inner.root.join(id.filename()))?;
        let parent = path
            .parent()
            .ok_or_else(|| VaultError::Denied(path.clone()))?;
        fsx::create_dir_all(&self.inner.guard, parent)?;
        fsx::create(&self.inner.guard, &path, initial.as_bytes())?;
        lock(&self.inner.writes).insert(path.clone(), (0, hash(initial.as_bytes())));
        self.inner.changed(&path, initial.as_bytes(), true);
        Ok(path)
    }
    /// Save an encoded attachment in the configured folder, using collision suffixes.
    pub fn write_attachment(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, VaultError> {
        crate::ops::write_attachment(&self.inner, name, bytes)
    }
    /// Copy a user-chosen external file into attachments with conflict handling.
    pub fn copy_attachment(&self, src: &Path) -> OpId {
        self.op(crate::ops::FileOp::CopyAttachment { source: src.into() })
    }
    /// Create a daily note from its optional template, or return the existing path.
    pub fn ensure_daily(&self, date: chrono::NaiveDate) -> Result<PathBuf, VaultError> {
        let id = nib_core::daily::path(&self.inner.cfg.vault, date)?;
        let path = self.inner.path(&self.inner.root.join(id.filename()))?;
        if path.exists() {
            return Ok(path);
        }
        let cfg = &self.inner.cfg.vault;
        let template = if cfg.daily_template.is_empty() {
            String::new()
        } else {
            let p = self.inner.path(
                &self
                    .inner
                    .root
                    .join(&cfg.templates)
                    .join(&cfg.daily_template),
            )?;
            std::fs::read_to_string(p)?
        };
        let vars = nib_core::template::TemplateVars {
            now: chrono::Local::now(),
            title: id.stem().into(),
            date_format: cfg.daily_format.clone(),
        };
        let text = nib_core::template::expand(&template, &vars);
        self.create_note(&id, &text)
    }
    /// Read a GTK 3 theme palette and recolor GTK 4's built-in base stylesheet.
    pub fn theme_css(name: &str) -> Option<String> {
        crate::theme::theme_css(name)
    }
}
