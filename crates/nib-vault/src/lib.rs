//! Guarded local filesystem services for nib, without GTK dependencies.
#![forbid(unsafe_code)]
pub mod fsx;
pub mod guard;
pub mod log;
pub mod state;
pub use guard::Guard;
/// Filesystem and policy failures presented as notices by the app.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// Operating system failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Confinement or containment refusal.
    #[error("refused path: {0}")]
    Denied(std::path::PathBuf),
    /// Invalid note data.
    #[error(transparent)]
    Text(#[from] nib_core::text::TextError),
    /// Invalid identity.
    #[error(transparent)]
    Id(#[from] nib_core::IdError),
    /// Recoverable service error.
    #[error("{0}")]
    Message(String),
}
/// Monotonically increasing successful-write token.
pub type Generation = u64;
