//! Pure note-domain logic; no filesystem access or application state.
#![forbid(unsafe_code)]
pub mod cli;
pub mod model;
pub mod settings;
pub mod text;
pub use model::*;
