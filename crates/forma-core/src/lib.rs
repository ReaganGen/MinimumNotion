//! Forma core library.
//!
//! Holds everything that touches durable data. It has no Tauri and no network
//! dependencies (ADR-017). Phase 1 contains only the atomic-write helper and
//! the app settings store. Documents, the index and collections arrive in
//! Phase 2 and later.

pub mod atomic;
pub mod error;
pub mod settings;
