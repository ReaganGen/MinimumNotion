//! Diaspar core library.
//!
//! Holds everything that touches durable data. It has no Tauri and no network
//! dependencies (ADR-017): settings, atomic writes, frontmatter and the
//! workspace (Document Service, scanning, history, trash, watcher), the
//! SQLite index and search, link extraction, collections, and the passages
//! and read-only view the AI assistant retrieves from.

pub mod archive;
pub mod atomic;
pub mod collection;
pub mod diagnostics;
pub mod error;
pub mod extract;
pub mod frontmatter;
pub mod index;
pub mod passages;
pub mod settings;
pub mod tokens;
pub mod workspace;
