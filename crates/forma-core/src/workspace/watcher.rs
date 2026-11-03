//! Watches the workspace for changes made outside Forma (ARCHITECTURE §4.9).
//! Events are only hints: the callback triggers a reconcile, which compares
//! files against the index. Forma's own writes match the index already, so
//! they produce no changes.

use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

pub struct Watcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

/// Calls `on_change` (from a background thread) when anything outside
/// `.forma/` changes, after 300 ms of quiet.
pub fn watch(root: &Path, on_change: impl Fn() + Send + 'static) -> notify::Result<Watcher> {
    let forma = root.join(".forma");
    let mut debouncer = new_debouncer(
        Duration::from_millis(300),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let relevant = events
                    .iter()
                    .flat_map(|e| e.paths.iter())
                    .any(|p| !p.starts_with(&forma));
                if relevant {
                    on_change();
                }
            }
            Err(errors) => {
                for err in errors {
                    log::warn!("file watcher error: {err}");
                }
            }
        },
    )?;
    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(Watcher {
        _debouncer: debouncer,
    })
}
