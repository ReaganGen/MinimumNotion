use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

/// Machine-readable error category sent across IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    Io,
    ReadOnly,
    Invalid,
    NotFound,
    Internal,
}

/// Error payload returned by every IPC command (ARCHITECTURE §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorPayload {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Errors from workspace operations. Messages are shown to the user, so they
/// say what happened in plain words.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0} wasn't found. It may have been moved or deleted outside Diaspar.")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    ReadOnly(String),
    #[error("couldn't access {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Maps an `io::Error` to `CoreError::Io` for `path`.
pub fn io_at(path: &Path) -> impl FnOnce(io::Error) -> CoreError + '_ {
    move |source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

impl From<&CoreError> for ErrorPayload {
    fn from(err: &CoreError) -> Self {
        let code = match err {
            CoreError::NotFound(_) => ErrorCode::NotFound,
            CoreError::Invalid(_) => ErrorCode::Invalid,
            CoreError::ReadOnly(_) => ErrorCode::ReadOnly,
            CoreError::Io { .. } => ErrorCode::Io,
        };
        // macOS asks before an app may use Documents, Desktop or Downloads.
        // If that was declined, say where to change it instead of "os error 1".
        if let CoreError::Io { path, source } = err
            && source.kind() == io::ErrorKind::PermissionDenied
        {
            return ErrorPayload::new(
                code,
                format!(
                    "Diaspar isn't allowed to use {}. Check the folder's permissions, or allow Diaspar \
                     in System Settings → Privacy & Security → Files and Folders.",
                    path.display()
                ),
            );
        }
        ErrorPayload::new(code, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_denied_folder_explains_where_to_allow_it() {
        let err = CoreError::Io {
            path: PathBuf::from("/Users/me/Documents/Diaspar"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        let payload = ErrorPayload::from(&err);
        assert!(payload.message.contains("Privacy & Security"));
        assert!(payload.message.contains("/Users/me/Documents/Diaspar"));
    }
}
