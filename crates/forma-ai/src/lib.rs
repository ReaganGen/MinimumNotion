//! Forma's AI provider layer (ARCHITECTURE §9). This is the only crate with
//! network access. It talks to providers, streams their answers and keeps
//! API keys in the Keychain. It never reads or writes the workspace.

pub mod error;
mod framing;
pub mod presets;
mod protocol;
pub mod provider;
pub mod secrets;
pub mod service;
mod tokens;
pub mod types;

pub use error::{AiError, AiErrorKind};
pub use service::{AiService, ChatStreamEvent, ConnectionReport};
