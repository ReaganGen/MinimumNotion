//! Token estimates for packing requests (ARCHITECTURE §11), on top of the
//! per-text estimate in `forma_core::tokens`.

pub use forma_core::tokens::estimate;

/// Per-message overhead for role markers and separators.
pub const PER_MESSAGE: u32 = 4;
