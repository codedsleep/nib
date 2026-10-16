//! Pure note-domain logic; no filesystem access or application state.
#![forbid(unsafe_code)]
pub mod cli;
pub mod model;
pub mod settings;
pub mod text;
pub use model::*;
pub mod frontmatter;
pub mod index;
pub mod links;
pub mod parse;
pub use index::Index;
pub use links::{link_text, rename_plan, resolve, shortest_link};
pub use parse::parse_note;
pub mod attach;
pub mod clip;
pub mod daily;
pub mod edit;
pub mod fuzzy;
pub mod launch;
pub mod palette;
pub mod render;
pub mod search;
pub mod sort;
pub mod template;
