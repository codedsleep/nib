//! Plain event values crossing the worker/main-context boundary.
use nib_core::{Edit, Index, NoteId, NoteMeta, sort::Entry, text::TextFormat};
use std::{path::PathBuf, sync::Arc};
/// Unique request token; discard events from replaced searches in the UI.
pub type RequestId = u64;
/// Unique file-operation token used when answering conflicts.
pub type OpId = u64;
/// Index construction or incremental patch.
#[derive(Debug, Clone)]
pub enum IndexEvent {
    /// At most one progress update per 50 ms.
    Progress(usize, usize),
    /// Whole initial index; move it onto the main context.
    Ready(Box<Index>),
    /// One parsed note to patch into the main-thread index.
    Upsert(Box<NoteMeta>),
    /// A vault-relative attachment appeared or changed.
    AttachmentUpsert(String),
    /// A vault-relative attachment disappeared.
    AttachmentRemove(String),
    /// A note disappeared.
    Remove(NoteId),
    /// Identity changed, followed by a reparse when available.
    Renamed(NoteId, NoteId),
}
/// Search match with a one-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Note identity.
    pub note: NoteId,
    /// One-based line.
    pub line: u64,
    /// Matching line, never logged.
    pub text: String,
}
/// A batch or terminal search message.
#[derive(Debug, Clone)]
pub enum SearchEvent {
    /// Consecutive hits are grouped by source note.
    Batch {
        /// Request identity.
        id: RequestId,
        /// A page of results.
        hits: Vec<SearchHit>,
    },
    /// The 2,000-result cap was reached.
    More(RequestId),
    /// No more results.
    Done(RequestId),
    /// Invalid regex or service failure.
    Failed(RequestId, String),
}
/// Finder walk result.
#[derive(Debug, Clone)]
pub enum WalkEvent {
    /// Relative file/folder paths, folders suffixed slash.
    Batch {
        /// Request identity.
        id: RequestId,
        /// Relative paths.
        paths: Vec<String>,
    },
    /// Completed traversal.
    Done(RequestId),
}
/// A recoverable notice and optional watcher fallback requirement.
#[derive(Debug, Clone)]
pub struct Notice {
    /// User-facing explanation.
    pub message: String,
    /// Re-list on focus-in until watches are restored.
    pub relist_on_focus: bool,
}
/// Answer applied to all conflicts in an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAnswer {
    /// Keep existing entries.
    Skip,
    /// Trash existing entries before replacing.
    Replace,
    /// Use a free ` (1)` destination name.
    Rename,
}
/// Successful operation totals, with buffer patches for the UI.
#[derive(Debug, Clone, Default)]
pub struct OpSummary {
    /// Number of changed top-level entries or files.
    pub files: usize,
    /// Rewritten outgoing links.
    pub links: usize,
    /// Exact edits to patch into open buffers without reloading.
    pub edits: Vec<Edit>,
    /// Where `CopyAttachment` put the file (a conflict may rename it).
    /// Additive, Goal B.
    pub created: Vec<PathBuf>,
}
/// File-operation progress and user questions.
#[derive(Debug, Clone)]
pub enum OpEvent {
    /// Progress notice, without a progress window.
    Progress {
        /// Operation identity.
        id: OpId,
        /// Completed entries.
        done: usize,
        /// Total entries.
        total: usize,
    },
    /// Answer once using `VaultHandle::answer`.
    Conflict {
        /// Operation identity.
        id: OpId,
        /// Existing destination.
        path: PathBuf,
    },
    /// Completed operation.
    Done {
        /// Operation identity.
        id: OpId,
        /// Changes and buffer patches.
        summary: OpSummary,
    },
    /// Refusal or I/O failure; original notes are retained.
    Failed {
        /// Operation identity.
        id: OpId,
        /// Error notice.
        message: String,
    },
}
/// A single event stream from a vault handle.
#[derive(Debug, Clone)]
pub enum VaultEvent {
    /// Cached or newly read folder rows.
    Listing {
        /// Absolute folder.
        folder: PathBuf,
        /// Shared immutable rows, including ordinary hidden files.
        entries: Arc<[Entry]>,
    },
    /// Initial or incremental index data.
    Index(IndexEvent),
    /// External note edit, filtered by content hash and self-write generation.
    NoteChanged {
        /// Absolute note path.
        path: PathBuf,
        /// Decoded buffer text.
        text: String,
        /// Original source format.
        fmt: TextFormat,
    },
    /// External non-UTF-8 edit must make an open buffer read-only.
    NoteInvalid {
        /// Absolute note path.
        path: PathBuf,
    },
    /// External removal.
    NoteRemoved {
        /// Old absolute path.
        path: PathBuf,
    },
    /// Paired external rename.
    NoteRenamed {
        /// Old absolute path.
        from: PathBuf,
        /// New absolute path.
        to: PathBuf,
    },
    /// Full-text results.
    Search(SearchEvent),
    /// Recursive finder paths.
    Walk(WalkEvent),
    /// File operation messages.
    Op(OpEvent),
    /// Recoverable service issue.
    Notice(Notice),
}
