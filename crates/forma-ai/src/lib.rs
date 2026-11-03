//! Forma's AI provider layer (ARCHITECTURE §9). This is the only crate with
//! network access. It talks to providers, streams their answers and keeps
//! API keys in the Keychain. It reads notes only through a read-only view
//! (`knowledge::Library`, ARCHITECTURE §10.1) and never writes the workspace.

pub mod error;
mod framing;
pub mod knowledge;
pub mod presets;
mod protocol;
pub mod provider;
pub mod secrets;
pub mod service;
mod tokens;
pub mod tools;
pub mod types;

pub use error::{AiError, AiErrorKind};
pub use service::{AiService, ChatStreamEvent, ConnectionReport};
