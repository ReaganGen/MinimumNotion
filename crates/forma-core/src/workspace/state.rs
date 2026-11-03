//! `.forma/workspace.json` (identity) and `.forma/tree.json` (sibling order
//! and favorites, advisory). DATA_MODEL §7.1–7.2.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic_staged;
use crate::error::{CoreResult, io_at};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreeState {
    #[serde(default = "format_version")]
    pub format_version: u32,
    /// Ordered child IDs per parent ID; the key `root` is the workspace root.
    #[serde(default)]
    pub order: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub favorites: Vec<String>,
}

fn format_version() -> u32 {
    FORMAT_VERSION
}

pub const ROOT_KEY: &str = "root";

pub fn order_key(parent: Option<&str>) -> String {
    parent.unwrap_or(ROOT_KEY).to_string()
}

pub fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save_json<T: Serialize>(path: &Path, value: &T, staging: &Path) -> CoreResult<()> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("state always serializes");
    bytes.push(b'\n');
    write_atomic_staged(path, &bytes, Some(staging)).map_err(io_at(path))
}
