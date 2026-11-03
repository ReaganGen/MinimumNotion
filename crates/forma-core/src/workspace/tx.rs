//! Transactions: several file changes that land together or not at all
//! (ARCHITECTURE §10.3, DATA_MODEL §7.6). While one is open, every file
//! action the Document Service takes (writing a file, renaming, making a
//! folder) is added to a journal on disk first and done second. If a step
//! fails, the recorded actions are undone at once, newest first; if Forma
//! stops halfway, the next start undoes them.
//!
//! Undoing an action checks the file first: a file that's no longer as the
//! action left it is left alone, and counted.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic_staged;
use crate::error::{CoreError, CoreResult, io_at};

use super::scan::{hash_rev, new_id};
use super::state::{FORMAT_VERSION, load_json, save_json};
use super::{Workspace, now_rfc3339};

/// One file action, with paths relative to the workspace root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum Action {
    /// A file written; `before_rev` is `None` when it was created.
    Write {
        page_id: Option<String>,
        path: String,
        before_rev: Option<String>,
        after_rev: String,
        /// The previous content, under `.forma/history/`.
        snapshot: Option<String>,
    },
    Rename {
        from: String,
        to: String,
    },
    MakeDir {
        path: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TxJournal {
    format_version: u32,
    journal_id: String,
    kind: String,
    started_at: String,
    pub actions: Vec<Action>,
}

pub(super) struct Tx {
    path: PathBuf,
    pub journal: TxJournal,
    /// Why snapshots taken in this transaction were taken: `ai` or `undo`.
    pub snapshot_reason: &'static str,
    /// Folders set aside to delete once the transaction commits.
    discarded: Vec<PathBuf>,
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

impl Workspace {
    pub(super) fn begin_tx(&mut self, kind: &str, snapshot_reason: &'static str) -> CoreResult<()> {
        if self.tx.is_some() {
            return Err(CoreError::Invalid(
                "Another change is still being applied.".into(),
            ));
        }
        let journal_id = new_id();
        let tx = Tx {
            path: self
                .forma()
                .join("journal")
                .join(format!("{journal_id}.json")),
            journal: TxJournal {
                format_version: FORMAT_VERSION,
                journal_id,
                kind: kind.into(),
                started_at: now_rfc3339(),
                actions: Vec::new(),
            },
            snapshot_reason,
            discarded: Vec::new(),
        };
        save_json(&tx.path, &tx.journal, &self.staging())?;
        self.tx = Some(tx);
        Ok(())
    }

    pub(super) fn in_tx(&self) -> bool {
        self.tx.is_some()
    }

    /// Adds `action` to the open journal, on disk, before it's done.
    pub(super) fn record(&mut self, action: Action) -> CoreResult<()> {
        let staging = self.staging();
        let Some(tx) = self.tx.as_mut() else {
            return Ok(());
        };
        #[cfg(test)]
        if self.crash_after_steps == Some(tx.journal.actions.len()) {
            // As if Forma stopped here: the journal stays for the next start.
            self.simulated_crash = true;
            return Err(CoreError::Invalid("simulated crash".into()));
        }
        tx.journal.actions.push(action);
        save_json(&tx.path, &tx.journal, &staging)
    }

    /// The `Write` actions recorded so far.
    pub(super) fn tx_writes(&self) -> Vec<Action> {
        self.tx
            .as_ref()
            .map(|tx| {
                tx.journal
                    .actions
                    .iter()
                    .filter(|a| matches!(a, Action::Write { .. }))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn commit_tx(&mut self) -> CoreResult<()> {
        let Some(tx) = self.tx.take() else {
            return Ok(());
        };
        fs::remove_file(&tx.path).map_err(io_at(&tx.path))?;
        for dir in tx.discarded {
            if let Err(err) = fs::remove_dir_all(&dir) {
                log::warn!("couldn't remove {dir:?}: {err}");
            }
        }
        Ok(())
    }

    /// Undoes everything the open transaction did, newest first.
    pub(super) fn abort_tx(&mut self) {
        #[cfg(test)]
        if self.simulated_crash {
            self.tx = None;
            return;
        }
        let Some(tx) = self.tx.take() else { return };
        let untouched = self.undo_actions(&tx.journal.actions);
        if untouched > 0 {
            log::warn!(
                "{untouched} file(s) changed during a failed change and were left as they are"
            );
        }
        let _ = fs::remove_file(&tx.path);
        self.rescan(None);
    }

    /// Undoes `actions` newest first. Returns how many files had changed
    /// since and were left alone.
    fn undo_actions(&mut self, actions: &[Action]) -> usize {
        let mut untouched = 0;
        let staging = self.staging();
        for action in actions.iter().rev() {
            match action {
                Action::Write {
                    path,
                    before_rev,
                    after_rev,
                    snapshot,
                    ..
                } => {
                    let file = self.root.join(path);
                    let current = fs::read(&file).ok().map(|b| hash_rev(&b));
                    if current.as_deref() == before_rev.as_deref() {
                        continue;
                    }
                    if current.as_deref() != Some(after_rev.as_str()) {
                        untouched += usize::from(current.is_some());
                        continue;
                    }
                    let restored = match snapshot {
                        None => fs::remove_file(&file).map_err(|e| e.to_string()),
                        Some(snapshot) => fs::read(self.root.join(snapshot))
                            .map_err(|e| e.to_string())
                            .and_then(|old| {
                                write_atomic_staged(&file, &old, Some(&staging))
                                    .map_err(|e| e.to_string())
                            }),
                    };
                    if let Err(err) = restored {
                        log::warn!("couldn't undo the write to {file:?}: {err}");
                        untouched += 1;
                    }
                }
                Action::Rename { from, to } => {
                    let (from, to) = (self.root.join(from), self.root.join(to));
                    if exists(&from) {
                        continue;
                    }
                    if !exists(&to) {
                        untouched += 1;
                        continue;
                    }
                    if let Err(err) = fs::rename(&to, &from) {
                        log::warn!("couldn't move {to:?} back to {from:?}: {err}");
                        untouched += 1;
                    }
                }
                Action::MakeDir { path } => {
                    // Only if empty: anything put there since stays.
                    let _ = fs::remove_dir(self.root.join(path));
                }
            }
        }
        untouched
    }

    /// Rolls back a transaction journal left by a crash (startup recovery).
    pub(super) fn roll_back_tx_journal(&mut self, path: &Path) {
        let journal: TxJournal = match load_json(path) {
            Ok(Some(journal)) => journal,
            _ => {
                log::warn!("unreadable journal {path:?} left in place");
                return;
            }
        };
        let untouched = self.undo_actions(&journal.actions);
        let what = if journal.kind == "operation_undo" {
            "An interrupted undo was rolled back."
        } else {
            "An interrupted AI change was undone; nothing of it was kept."
        };
        let mut notice = what.to_string();
        if untouched > 0 {
            notice.push_str(&format!(
                " {untouched} file(s) had changed since and were left as they are."
            ));
        }
        self.notices.push(notice);
        let _ = fs::remove_file(path);
    }

    // ------------------------------------------------ actions, journaled

    pub(super) fn rename_path(&mut self, from: &Path, to: &Path) -> CoreResult<()> {
        self.record(Action::Rename {
            from: rel(&self.root, from),
            to: rel(&self.root, to),
        })?;
        fs::rename(from, to).map_err(io_at(from))
    }

    pub(super) fn make_dir(&mut self, dir: &Path) -> CoreResult<()> {
        self.record(Action::MakeDir {
            path: rel(&self.root, dir),
        })?;
        fs::create_dir(dir).map_err(io_at(dir))
    }

    /// Deletes a folder. In a transaction it's set aside until the
    /// transaction commits, so it can be put back.
    pub(super) fn discard_dir(&mut self, dir: &Path) -> CoreResult<()> {
        if !self.in_tx() {
            return fs::remove_dir_all(dir).map_err(io_at(dir));
        }
        let aside = self.staging().join(format!("discard-{}", new_id()));
        self.rename_path(dir, &aside)?;
        if let Some(tx) = self.tx.as_mut() {
            tx.discarded.push(aside);
        }
        Ok(())
    }

    /// Writes a new `.forma/` record, such as a trash entry, journaled.
    pub(super) fn save_record<T: Serialize>(&mut self, path: &Path, value: &T) -> CoreResult<()> {
        if self.in_tx() {
            let mut bytes = serde_json::to_vec_pretty(value).expect("records always serialize");
            bytes.push(b'\n');
            let before = fs::read(path).ok();
            if before.is_some() {
                return Err(CoreError::Invalid(format!(
                    "{} already exists.",
                    rel(&self.root, path)
                )));
            }
            self.record(Action::Write {
                page_id: None,
                path: rel(&self.root, path),
                before_rev: None,
                after_rev: hash_rev(&bytes),
                snapshot: None,
            })?;
            return write_atomic_staged(path, &bytes, Some(&self.staging())).map_err(io_at(path));
        }
        save_json(path, value, &self.staging())
    }

    /// Records a page file write in the open transaction, snapshotting the
    /// previous content (if any) so it can be put back.
    pub(super) fn record_page_write(
        &mut self,
        id: &str,
        file: &Path,
        text: &str,
        previous: Option<(&str, &str)>,
    ) -> CoreResult<Option<PathBuf>> {
        let old: Option<(String, String)> = match previous {
            Some((text, rev)) => Some((text.to_string(), rev.to_string())),
            None => fs::read(file)
                .ok()
                .map(|b| (String::from_utf8_lossy(&b).into_owned(), hash_rev(&b))),
        };
        let reason = self
            .tx
            .as_ref()
            .map(|t| t.snapshot_reason)
            .unwrap_or("batch");
        let snapshot = match &old {
            Some((text, _)) => Some(self.take_snapshot(id, text, reason)?),
            None => None,
        };
        self.record(Action::Write {
            page_id: Some(id.to_string()),
            path: rel(&self.root, file),
            before_rev: old.map(|(_, rev)| rev),
            after_rev: hash_rev(text.as_bytes()),
            snapshot: snapshot.as_deref().map(|s| rel(&self.root, s)),
        })?;
        Ok(snapshot)
    }
}
