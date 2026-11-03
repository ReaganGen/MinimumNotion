//! Forma core library.
//!
//! Holds everything that touches durable data. It has no Tauri and no network
//! dependencies (ADR-017): settings, atomic writes, frontmatter and the
//! workspace (Document Service, scanning, history, trash, watcher), the
//! SQLite index and search, link extraction, and collections.

pub mod atomic;
pub mod collection;
pub mod error;
pub mod extract;
pub mod frontmatter;
pub mod index;
pub mod settings;
pub mod workspace;
