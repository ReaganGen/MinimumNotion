//! Forma core library.
//!
//! Holds everything that touches durable data. It has no Tauri and no network
//! dependencies (ADR-017): settings, atomic writes, frontmatter and the
//! workspace (Document Service, scanning, history, trash, watcher).
//! Collections and search arrive in Phase 3.

pub mod atomic;
pub mod error;
pub mod frontmatter;
pub mod settings;
pub mod workspace;
